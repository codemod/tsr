# checker_types triage: types-any-triage + type-operators(T) + contextual(T)

Tree: working tree as of 2026-10-05 00:41 (main between `cfa02cf4` and `a0fa106e`; release `verdictdump` rebuilt at that point). Upstream: `vendor/typescript-go` @ `5b1047d`.

Population: **730 cases** (692 `types-any-triage` + 33 `type-operators` T + 5 `contextual` T), **3806 aligned non-RIGHT lines** (GAP 1233, WRONG 2573). All 730 cases still fail at this tree. 23 cases also carry unaligned lines (listed at the end); they cannot be finished by any cluster below on its own and are excluded from every "finished alone" count.

## Method

1. `cargo run --release -p tsr-conformance --example verdictdump` (plus `TSR_VERDICT_EXPR=1` for the expression column) gave every aligned line's verdict. `gapdump` and `any_audit` (`TSR_ANY_DUMP=1`) supplied hint strings only.
2. **Wave 1:** 14 classifiers each took about 52 cases and keyed every non-RIGHT line to the upstream operation whose faithful port makes it RIGHT. They minimised cases with `probefile` and checked most new keys against a real tsgo oracle (a copy of vendor tsgo running `TestLocal` on ad-hoc files). That produced 490 raw keys, which were merged into about 60 families.
3. **Wave 2:** 19 verifiers each opened members of 1–5 families. They split every family that was really a list of symptoms (conventions: *a cluster named by its symptom is a list of leads*), and checked 2–3 examples end-to-end: source, baseline line, TSR line or probe, and the tsgo code. They grep-verified the TSR function and wrote a per-line map. Clusters below marked **V** passed this step. Clusters marked **1** (the long tail) have only the wave-1 diagnosis, usually from one minimised probe plus an oracle run.
4. Scoring is per line. A case counts as **blocked** by every cluster that owns at least one of its lines. It counts as **finished alone** by cluster X only when all of its non-RIGHT lines map to X and it has no unaligned lines. This is level 4 in `docs/conventions.md`: what else in the same case still fails.

Raw artifacts (not tracked): `/tmp/triage/out_*.jsonl` (wave-1 per-line keys), `/tmp/triage/verify_G*.json` (wave-2 clusters, examples, plans, line maps), `/tmp/verdicts_expr.tsv`.

## Port sets (clusters that share one prerequisite, scored together)

| port set | member clusters | cases blocked | finished if the whole set lands | lines |
|---|---|---|---|---|
| CLASS-BASE (all consumers) | `CLASS-GET-BASE-TYPES`, `CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES`, `SUPER-FROM-BASE-TYPES`, `STATIC-SIDE-FROM-BASE-CONSTRUCTOR-TYPE`, `CLASS-TYPE-BASE-TYPE-VARIABLE-INTERSECTION`, `EXPORT-ASSIGNMENT-CLASS-EXPRESSION-TARGET` | 47 | 34 | 127 |
| TYPE-ALIAS attribute (all arms) | `TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION`, `TYPE-ALIAS-INSTANTIATION-NEW-ALIAS`, `DEFERRED-TYPE-REFERENCE-ALIAS`, `INSTANTIATE-MAPPED-TYPE-ALIAS-DROP`, `CONDITIONAL-TYPE-ROOT-ALIAS`, `IMPORTED-ALIAS-REFERENCE-WRITTEN-ARGS` | 69 | 40 | 251 |
| ITERATION-TYPES engine + generators | `YIELD-STAR-ITERATION-TYPES-OF-ITERABLE`, `ASYNC-YIELD-STAR-ITERATION-TYPES`, `GENERATOR-ANNOTATION-ITERATION-TYPES`, `CONTEXTUAL-RETURN-GENERATOR-FILTER`, `YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE`, `IMPORT-CALL-ARGUMENT-CONTEXT`, `CREATE-GENERATOR-TYPE-EMPTY-FALLBACK`, `CONTEXTUAL-RETURN-IIFE-ARM` | 32 | 31 | 185 |
| QUALIFIED/alias type references | `QUALIFIED-NAME-LEFT-ALIAS-RESOLVE`, `QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE`, `IMPORT-EQUALS-ALIAS-TYPEREF-TARGET`, `TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL` | 39 | 25 | 175 |
| UNTYPED-CALL (both) | `UNTYPED-CALL-ANY-CALLEE`, `UNTYPED-CALL-FUNCTION-TYPED-CALLEE`, `UNTYPED-CALL-ARGUMENT-ANY-CONTEXT` | 26 | 23 | 61 |

## Ranked table: verified clusters (V)

Ranked by cases finished alone, then by cases blocked.

| # | cluster | root cause | tsgo (pinned) | TSR to change | blocked | finished alone | lines |
|---|---|---|---|---|---|---|---|
| 1 | `CLASS-GET-BASE-TYPES` | The writer's heritage line is GetTypeAtLocation(EWTA) = getTypeOfNode's class-extends arm = getTypeWithThisArgument(getBaseTypes(classType)[0]) (errorType -> writer falls back to the expression's type); TSR has no getBas | internal/checker/checker.go:31959 getTypeOfNode (class-extends EWTA arm) -> :19167 getBaseTypes -> :19220 resolveBaseTypesOfClass -> :16957 getBaseConstructorTy | crates/tsr-conformance/src/types_producer.rs:752 type_id_at_location_tracking heritage arm 1 (heritage_base_symbol :263, declared.rs:149 base_type_of_heritage_e | 45 | 23 | 71 |
| 2 | `UNTYPED-CALL-ANY-CALLEE` | resolveCallExpression/resolveNewExpression treat ANY callee type (IsTypeAny(funcType), and resolveNewExpression's IsTypeAny(apparent)) as an untyped call answering anySignature->any; TSR's is_untyped_call_target admits o | internal/checker/checker.go:9933 isUntypedFunctionCall (IsTypeAny(funcType) disjunct, called from resolveCallExpression checker.go:8529; new: resolveNewExpressi | crates/tsr-checker/src/calls.rs:2113 is_untyped_call_target (+ any_is_written_in_an_annotation calls.rs:2218; callers calls.rs:683 call arm, expressions.rs:2475 | 16 | 16 | 35 |
| 3 | `TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION` | TSR has no TypeAlias attribute on types: a generic alias's declared type is minted as an opaque nominal `Name<Params>` (declared.rs:6048) and every reference is an opaque (symbol,args) pair via create_type_reference, wit | internal/checker/checker.go:23837 getDeclaredTypeOfTypeAlias; :23641 getTypeAliasInstantiation -> :22104 instantiateTypeWithAlias (alias sources: :23719 getAlia | crates/tsr-checker/src/declared.rs:5963 get_declared_type_of_type_alias (mint at :6048) + crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_referenc | 30 | 15 | 106 |
| 4 | `TYPE-ALIAS-INSTANTIATION-NEW-ALIAS` | getTypeFromTypeAliasReference's newAliasSymbol arm (outer non-local alias whose body is directly a generic-alias reference) is not ported: TSR instantiates the inner alias as an alias-blind named reference `Inner<args>`  | internal/checker/checker.go:23580 getTypeFromTypeAliasReference (newAliasSymbol 23609-23616) -> :23641 getTypeAliasInstantiation -> :22104 instantiateTypeWithAl | crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference (final create_type_reference arm at :4377; cache in declared.rs:5157 create_type_referen | 29 | 13 | 93 |
| 5 | `QUALIFIED-NAME-LEFT-ALIAS-RESOLVE` | resolveQualifiedName resolves the LEFT of `X.Y` with resolveEntityName(left, Namespace), whose alias loop (resolveAlias) follows import-equals / `import * as` / ES-import / default-import aliases to the namespace or modu | internal/checker/checker.go:15828 resolveQualifiedName (left via resolveEntityName at :15829; alias loop :15821; re-export fallback :15853) ; internal/checker/c | crates/tsr-checker/src/declared.rs:4841 resolve_entity_name (QualifiedName arm; miss lands in qualified_type_reference at declared.rs:4695 -> unresolved_type_re | 18 | 12 | 90 |
| 6 | `SYNTHETIC-DEFAULT-IMPORT-TARGET` | getTargetOfModuleDefault's `hasSyntheticDefault \|\| hasDefaultOnly` arm (resolve the default import to resolveExternalModuleSymbol(module), i.e. the export= target or the module itself) is ported only for narrow slices: T | internal/checker/checker.go:14536 getTargetOfModuleDefault (arm :14578-14585), checker.go:14818 canHaveSyntheticDefault (node arm :14823-14848), checker.go:1480 | crates/tsr-checker/src/symbols.rs:1386 module_default_target (with crates/tsr-checker/src/check.rs:2660 can_have_synthetic_default) | 18 | 12 | 58 |
| 7 | `NULL-WIDENING-TYPES-MISSING` | With strictNullChecks off, null/undefined expressions are nullWideningType/undefinedWideningType (ContainsWideningType) and addTypesToUnion drops nullables so an all-nullable union returns the widening null; TSR has no n | internal/checker/checker.go:25027 createWideningType (+ :990 nullWideningType, :25653 getUnionTypeWorker empty-typeSet arm, :13144 checkObjectLiteral, :18400 ge | crates/tsr-checker/src/objects.rs:867 check_object_literal_members (refusal at :1734); crates/tsr-checker/src/unions.rs:570 union_type_worker (gap at :697); cra | 13 | 12 | 83 |
| 8 | `MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT` | A deferred conditional type is a ConditionalType object upstream, printed by the node builder from its instantiated parts (defaults filled, qualified names via symbol chain). TSR mints it from written_type_text, which de | internal/checker/checker.go:24269 getTypeFromConditionalTypeNode ; internal/checker/checker.go getTypeFromMappedTypeNode / getTypeFromConditionalTypeNode, print | crates/tsr-checker/src/declared.rs:447 get_type_from_type_node MappedTypeNode\|ConditionalTypeNode arm (written_type_text at :494; signatures.rs:5264 written_typ | 28 | 10 | 135 |
| 9 | `REORDER-CANDIDATES` | resolveCall's reorderCandidates (hoist literal-typed 'specialized' signatures; splice later declaration groups of a merged symbol ahead of earlier ones) is ported only for the construct road; the call road keeps declarat | internal/checker/checker.go:8957 reorderCandidates (called from :8843 resolveCall) | crates/tsr-checker/src/calls.rs:2415 subtype_pass_outcome (literal guard L2432) + calls.rs:1720 choose_overload / calls.rs:1416 resolve_call_signature_at; exist | 12 | 10 | 90 |
| 10 | `ASYNC-YIELD-STAR-ITERATION-TYPES` | getIterationTypesOfIterable with IterationUseAsyncYieldStar (async iterable first, then sync iterable via getAsyncFromSyncIterationTypes) is unported, so every yield* inside an async generator answers error/next-slot for | internal/checker/checker.go:10952 checkYieldExpression (use = AsyncYieldStar) -> :6287 getIterationTypesOfIterableWorker (async arm) -> :6436 getAsyncFromSyncIt | crates/tsr-checker/src/expressions.rs:3456 check_yield_expression (is_async gates 3533-3545, 3576-3589); crates/tsr-checker/src/signatures.rs:2497 return_type_f | 10 | 9 | 68 |
| 11 | `GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE` | getTypeOfNode only evaluates an identifier as a value when ast.IsExpressionNode is true and otherwise falls through to errorType; TSR's type_id_at_location_tracking ends with an ungated check_expression (plus a label arm | internal/checker/checker.go:31955 getTypeOfNode (IsExpressionNode gate; fallthrough return c.errorType at :32035) | crates/tsr-conformance/src/types_producer.rs:1577 type_id_at_location_tracking (final check_expression fallthrough; also label arm :1404 and qualified-name-left | 13 | 8 | 36 |
| 12 | `CLONE-BINDING-NAME` | parameterToParameterDeclarationName -> cloneBindingName clones the WRITTEN binding pattern for every shape: holes, string-literal/computed property names, and the elements list's trailing comma, dropping only initializer | internal/checker/nodebuilderimpl.go:1713 cloneBindingName (via :1691 parameterToParameterDeclarationName) ; internal/checker/nodebuilderimpl.go:1691 parameterTo | crates/tsr-checker/src/signatures.rs:4625 render_binding_pattern (called from parameter_of :4678) ; crates/tsr-checker/src/signatures.rs:4625 render_binding_pat | 12 | 8 | 45 |
| 13 | `TYPE-ONLY-IMPORT-EXPORT-NAME-DECLARED-TYPE` | getTypeOfNode's IsTypeDeclarationName arm answers getDeclaredTypeOfSymbol(alias) unconditionally (error -> printed any) for the name of a type-only ImportClause/ImportSpecifier/ExportSpecifier. TSR's arm covers only spec | internal/checker/checker.go:31975 getTypeOfNode (IsTypeDeclarationName arm; ast/utilities.go:3585 IsTypeDeclaration) -> checker.go:24094 getDeclaredTypeOfAlias | crates/tsr-conformance/src/types_producer.rs:818 type_id_at_location_tracking (type-only specifier arm) | 9 | 8 | 10 |
| 14 | `SIGNATURE-DECLARATION-ANNOTATION-REUSE` | When printing a signature's parameter/return type, upstream serializeTypeForDeclaration/serializeReturnTypeForSignature reuse the WRITTEN annotation node whenever the pseudochecker's type for it is equivalent to the symb | internal/checker/nodebuilderimpl.go:2181 serializeTypeForDeclaration (reuse arm :2231-2256; equivalence gate pseudotypenodebuilder.go:362 pseudoTypeEquivalentTo | crates/tsr-checker/src/signatures.rs:4945 written_annotation_text (callers: parameter_of :4863, return :1565) | 16 | 7 | 62 |
| 15 | `FORIN-VARIABLE-INDEX-TYPE` | The for-in arm of getTypeForVariableLikeDeclaration checks the iterated expression and answers getExtractStringType(getIndexType(nonNullable(exprType))) when that index type is a TypeParameter/Index type (and inherits ci | internal/checker/checker.go:16658 getTypeForVariableLikeDeclaration (ForInStatement arm, -> checker.go:26709 getExtractStringType) | crates/tsr-checker/src/symbols.rs:5174 get_type_for_variable_like_declaration (for-in arm returns intrinsics.string at :5179) | 9 | 7 | 53 |
| 16 | `YIELD-STAR-ITERATION-TYPES-OF-ITERABLE` | TSR has no port of getIterationTypesOfIterable returning all three iteration types (yield/return/next); checkYieldExpression's yield* arm and checkAndAggregateYieldOperandTypes' yield* arm are replaced by shape tests (no | internal/checker/checker.go:10952 checkYieldExpression (yield* return arm :10998-11001 -> :6238 getIterationTypeOfIterable -> :6265 getIterationTypesOfIterable  | crates/tsr-checker/src/expressions.rs:3456 check_yield_expression (yield* arms 3526-3563, 3575-3654); crates/tsr-checker/src/signatures.rs:2497 return_type_from | 8 | 7 | 21 |
| 17 | `MODULE-AUGMENTATION-MERGE` | mergeModuleAugmentation (non-global arm) is not ported: `declare module "x"` augmentations in module files (and nested ones in ambient modules) are never merged into resolveExternalModuleSymbol(mainModule); TSR only merg | internal/checker/checker.go:1397 mergeModuleAugmentation (non-global arm :1408-1448) | crates/tsr-binder/src/binder.rs:653 merge_globals (only global augmentations; non-global listed as not merged in merge_symbol doc :803, merge_symbol at :819) | 9 | 6 | 73 |
| 18 | `APPEND-LOCAL-TYPE-PARAMETERS` | getLocalTypeParametersOfClassOrInterfaceOrTypeAlias/appendTypeParameters collect type parameters from EVERY class/interface/alias declaration of a merged symbol, deduped by merged type-parameter symbol (AppendIfUnique) w | internal/checker/checker.go:23810 appendLocalTypeParametersOfClassOrInterfaceOrTypeAlias (+ :23822 appendTypeParameters, :22007 getResolvedTypeParameterDefault) | crates/tsr-checker/src/declared.rs:7922 local_type_parameters_of (also signatures.rs:5754 type_parameter_of for function type-param lists) | 9 | 6 | 32 |
| 19 | `DEFERRED-TYPE-REFERENCE-ALIAS` | isDeferredTypeReferenceNode's alias arm is not ported for class/interface references and array type nodes: a reference/array node that is directly a type-alias body must become createDeferredTypeReference carrying getAli | internal/checker/checker.go:23236 isDeferredTypeReferenceNode (alias arm 23237) <- :23200 getTypeFromClassOrInterfaceReference and :24121 getTypeFromArrayOrTupl | crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference (class/interface/Array<T>/ReadonlyArray<T> targets, final create_type_reference at :4377 | 9 | 6 | 27 |
| 20 | `CONTEXTUAL-SIGNATURE-TYPEPARAM-ADOPTION` | assignContextualParameterTypes copies a GENERIC contextual signature's typeParameters onto the context-sensitive function's signature (sig.typeParameters = context.typeParameters); TSR never adopts them, so the function  | internal/checker/checker.go:10349 assignContextualParameterTypes (adoption at :10350-10356; caller :10152 contextuallyCheckFunctionExpressionOrObjectLiteralMeth | crates/tsr-checker/src/signatures.rs:1325 get_signature_from_declaration (type params only from own syntax) + signatures.rs:6201 get_type_of_function_expression | 7 | 6 | 30 |
| 21 | `CLASS-EXPRESSION-SELF-NAME-RESOLVE` | NameResolver's ClassExpression arm that resolves a named class expression's own name (meaning includes Class) to the class-expression symbol inside its body is not ported in TSR's binder resolve_name walk. | internal/binder/nameresolver.go:189 NameResolver.Resolve (IsClassExpression self-name arm under case KindClassExpression :170) | crates/tsr-binder/src/lib.rs:730 resolve_name_excluding_with_export_alias (Class/Interface member-lookup arm; add self-name check after it, mirroring the Functi | 6 | 6 | 46 |
| 22 | `OBJLIT-THIS-LITERAL-SELF-FALLBACK` | getContextualThisParameterType's object-literal arm (noImplicitThis or JS) falls back to getWidenedType(checkExpressionCached(containingLiteral)) when the literal has no contextual type; TSR's contextual_object_this_type | internal/checker/checker.go:12021 getContextualThisParameterType (fallback :12049-12054) | crates/tsr-checker/src/contextual.rs:611 contextual_object_this_type (early return at :631) | 6 | 6 | 16 |
| 23 | `SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS` | getAccessibleSymbolChain's per-table trySymbolTable walk (innermost scope first; direct/ExportSymbol hit sorted with alias candidates by compareSymbols) is not ported for same-file `import x = Entity` aliases: TSR best_n | internal/checker/symbolaccessibility.go:535 trySymbolTable (via :373 getAccessibleSymbolChain, nodebuilderimpl.go:1087 getSymbolChain) | crates/tsr-checker/src/checker.rs:3537 best_name (admit_local_import_equals gate at :3619); also :3725 own_name_alias_at / :2443 qualified_name_at for the type- | 22 | 5 | 63 |
| 24 | `QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE` | After a qualified name resolves, tsgo answers getTypeReferenceType(symbol): getTypeFromTypeAliasReference gives the alias's declared type or instantiation (intrinsic bodies print `number`/`string`/`unique symbol`, generi | internal/checker/checker.go:23146 getTypeReferenceType (-> :23580 getTypeFromTypeAliasReference / :23169 getTypeFromClassOrInterfaceReference, :21954 fillMissin | crates/tsr-checker/src/declared.rs:4651 qualified_type_reference (argument-less mint :4741-4762, generic mint :4768-4818, fall-to-unresolved :4819) | 14 | 5 | 67 |
| 25 | `TYPE-ALIAS-ACCESSIBILITY-GATE` | typeToTypeNode's alias arm uses an alias name only when IsTypeSymbolAccessible(alias, enclosingDeclaration); TSR decides alias naming at type creation with a syntactic local-scope check that is skipped for generic alias  | internal/checker/nodebuilderimpl.go:3362 typeToTypeNode alias arm -> symbolaccessibility.go:11 IsTypeSymbolAccessible | crates/tsr-checker/src/declared.rs:3594 alias_symbol_for_type_node / :1165 alias_declaration_is_locally_scoped (+ printing at checker.rs:1859 type_to_string_at) | 8 | 5 | 53 |
| 26 | `TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL` | When a type reference's name binds to an alias whose target is unknownSymbol (missing module, unexported member) or has no Type meaning (module, instantiated namespace, `import * as`), tsgo's resolveTypeReferenceName fai | internal/checker/checker.go:23087 resolveTypeReferenceName (-> :23102 getUnresolvedSymbolForEntityName; meaning filter getSymbol :2176, getSymbolFlagsEx unknown | crates/tsr-checker/src/declared.rs:1397 get_type_from_type_reference alias arms: §157 `return error` :1504, non-TYPE target `return error` :1606, module_specifi | 8 | 5 | 16 |
| 27 | `RESOLVE-ES-MODULE-SYMBOL-CLONE` | resolveESModuleSymbol's namespace-import wrapping is ported only for the hasSignatures class/function arm (module_clone_type); the isEsmCjsRef / `default`-property arms that build `{ default: T }` via getTypeWithSyntheti | internal/checker/checker.go:15568 resolveESModuleSymbol (arm :15609-15618), :15646 getTypeWithSyntheticDefaultImportType, :15707 createDefaultPropertyWrapperFor | crates/tsr-checker/src/symbols.rs:600 module_clone_type (called from get_type_of_alias symbols.rs:409) ; crates/tsr-checker/src/symbols.rs:600 module_clone_type | 7 | 5 | 23 |
| 28 | `UNTYPED-CALL-ARGUMENT-ANY-CONTEXT` | Arguments of an untyped/error call (resolveUntypedCall/resolveErrorCall, super() with any/error super type) are checked while the call's signature is resolvingSignature/anySignature/unknownSignature, so getContextualType | internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex (resolvingSignature/anySignature have no parameters -> relater.go:1757 getTypeAtPosition r | crates/tsr-checker/src/contextual.rs:2446 contextual_type_for_argument_resolving (no untyped-call arm) and crates/tsr-checker/src/signatures.rs:6149 get_type_of | 7 | 5 | 22 |
| 29 | `AMBIENT-MODULE-BIND-VALUEMODULE` | bindModuleDeclaration declares every ambient module that is not an external augmentation (string-named `declare module "x"`, or `declare global` in a script) as ValueModule unconditionally; TSR picks NAMESPACE_MODULE fro | internal/binder/binder.go:771 bindModuleDeclaration (ambient arm :773-781) | crates/tsr-binder/src/binder.rs:4931 symbol-flag table arm Node::ModuleDeclaration | 7 | 5 | 9 |
| 30 | `RESOLVE-NAME-EXPORT-DEFAULT-LOCAL` | NameResolver's module arm first checks moduleExports['default'] and accepts it when GetLocalSymbolForExportDefault(result).Name == name; TSR's binder exports arm has no default-local check, so the local name of `export d | internal/binder/nameresolver.go:113 Resolve (export-default local-name arm, 110-120) | crates/tsr-binder/src/lib.rs:784 resolve_name_excluding_with_export_alias exports arm (add default arm before 802) | 6 | 5 | 30 |
| 31 | `LOGICAL-OR-COALESCE-GENERIC-GATE` | `a \|\| b` / `a ?? b` build getUnionTypeEx([nonNullable(left), right], UnionReductionSubtype); TSR check_logical_or_coalescing returns error whenever a constituent is a type parameter or unknown (or a reference with such a | internal/checker/checker.go:12509 checkBinaryLikeExpressionWorker BarBar arm (:12518 QuestionQuestion arm) -> :25934 removeSubtypes | crates/tsr-checker/src/binary.rs:452 check_logical_or_coalescing (undecidable gate at :508-516) | 6 | 5 | 16 |
| 32 | `FUNCEXPR-NIL-CONTEXTUAL-SIGNATURE-POSITIONS` | Upstream: getContextualSignature nil -> assignNonContextualParameterTypes (implicit any) for any position without a contextual type; TSR instead requires its syntactic has_no_contextual_type whitelist to PROVE absence an | internal/checker/checker.go:10395 assignNonContextualParameterTypes (from :10152 contextuallyCheckFunctionExpressionOrObjectLiteralMethod when getContextualSign | crates/tsr-checker/src/signatures.rs:4406 has_no_contextual_type (+ :4055 declaration_takes_no_contextual_return lacks GetAccessor arm) feeding :6149 get_type_o | 5 | 5 | 53 |
| 33 | `CANDIDATE-FOR-OVERLOAD-FAILURE` | When chooseOverload rejects every candidate, resolveCall still returns getCandidateForOverloadFailure -> pickLongestCandidateSignature, instantiated by inferSignatureInstantiationForOverloadFailure (getInferredType defau | internal/checker/checker.go:9498 getCandidateForOverloadFailure (-> :9510 pickLongestCandidateSignature, :9575 inferSignatureInstantiationForOverloadFailure, :9 | crates/tsr-checker/src/calls.rs:624 check_call_expression_worker (failure branch L693-729) + crates/tsr-checker/src/expressions.rs:2430 check_new_expression (si | 5 | 5 | 10 |
| 34 | `LATE-BIND-INDEX-SIGNATURE-TYPE-LITERAL` | Type-literal members with a computed name follow getResolvedMembersOrExportsOfSymbol's switch: late-bindable name -> real member, late-bindable index signature (entity-name expr assignable to string\|number\|symbol, incl.  | internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (:19662 hasLateBindableIndexSignature arm; :19976 isLateBindableIndexSignature requires isLateBinda | crates/tsr-checker/src/declared.rs:1935 build_type_literal (method computed arm :2050-2054 `None => return error`; property computed arm :2305-2343 incl. `let S | 5 | 5 | 6 |
| 35 | `RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM` | getTypeOfNode's isInRightSideOfImportOrExportAssignment arm resolves every entity-name part with getSymbolAtLocation: Namespace meaning and dontResolveAlias for a bare identifier or qualified-name left of `import x = …`, | internal/checker/checker.go:32016 getTypeOfNode (isInRightSideOfImportOrExportAssignment arm; utilities.go:1107) + checker.go:14474 getSymbolOfPartOfRightHandSi | crates/tsr-conformance/src/types_producer.rs:1244 type_id_at_location_tracking (import-equals qualified-left arm; no bare-identifier arm) and :1147 (export-assi | 11 | 4 | 11 |
| 36 | `CONTEXTUAL-RETURN-IIFE-ARM` | getContextualReturnType's last arm returns getContextualType(iife call) for an immediately invoked function; TSR's ReturnStatement arm (and yield operand path) stops at contextual_signature(function)? with no IIFE fallba | internal/checker/checker.go:29690 getContextualReturnType (iife arm) | crates/tsr-checker/src/contextual.rs:1430 get_contextual_type ReturnStatement arm (+ yield-operand contextual return path); immediately_invoked_call exists at c | 6 | 4 | 61 |
| 37 | `EMPTY-NAMED-FUNCTION-SYMBOL-TYPE` | getTypeOfFuncClassEnumModuleWorker gives a method/function symbol an anonymous object type regardless of its name; TSR's worker gates on has_a_name_no_type_query_can_spell (name.is_empty()) before distinguishing typeof-s | internal/checker/checker.go:16912 getTypeOfFuncClassEnumModuleWorker | crates/tsr-checker/src/symbols.rs:2732 get_type_of_func_class_enum_module_worker (gate at :2751 via :3030 has_a_name_no_type_query_can_spell) | 6 | 4 | 6 |
| 38 | `MAPPED-MEMBER-KEYS-ENUM-AND-ANY` | resolveMappedTypeMembers' key enumeration: TSR's mapped_member_keys rejects enum-literal keys (only STRING_LITERAL\|NUMBER_LITERAL admitted) and has no `any` modifiers-type arm (forEachMappedTypePropertyKeyTypeAndIndexSig | internal/checker/checker.go:20894 resolveMappedTypeMembers; :22727 forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType (any arm :22731) | crates/tsr-checker/src/mapped.rs:481 mapped_member_keys (L576 literal-flag admission, L587 `return None`; L495-545 homomorphic source without any arm) | 5 | 4 | 26 |
| 39 | `IMPORT-TYPE-NODE-TYPE-MEANING` | getTypeFromImportTypeNode with type meaning is ported only for a qualified reference without type arguments; written type arguments (`import("./id").Id<X>`) and the unqualified form (`import("./foo")` of an export= class | internal/checker/checker.go:24575 getTypeFromImportTypeNode (qualifier walk :24594-24633, unqualified :24634-24644) -> :24657 resolveImportSymbolType -> getType | crates/tsr-checker/src/declared.rs:4600 get_type_from_import_type_node | 5 | 4 | 24 |
| 40 | `IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET` | `import y = x` where x is itself an alias: resolveEntityName(Namespace, dontResolveAlias=true) accepts an alias whose chain carries Namespace meaning and returns it; TSR's Identifier arm rejects ALIAS symbols unless thei | internal/checker/checker.go:14474 getSymbolOfPartOfRightHandSideOfImportEquals (via 14439 getTargetOfImportEqualsDeclaration; resolveEntityName 15772) | crates/tsr-checker/src/symbols.rs:1124 resolve_alias ModuleReference::Identifier arm (ALIAS branch 1148-1165) | 5 | 4 | 22 |
| 41 | `TYPE-LITERAL-PROPERTY-ANNOTATION-REUSE` | addPropertyToElementList serializes each property of a type-literal/interface type through serializeTypeForDeclaration on the PropertySignature, reusing the written annotation node when it is equivalent (or the type is e | internal/checker/nodebuilderimpl.go:2486 addPropertyToElementList -> :2181 serializeTypeForDeclaration | crates/tsr-checker/src/declared.rs:1935 build_type_literal (member `printed` at :2405) | 5 | 4 | 10 |
| 42 | `BINDING-ELEMENT-ALLOW-MISSING-DEFAULT` | getBindingElementTypeFromParentType indexes with AccessFlagsAllowMissing when the element has a default, so a property absent from the parent yields undefined and the element becomes the default's type; TSR's destructuri | internal/checker/checker.go:17719 getBindingElementTypeFromParentType (accessFlags AllowMissing when hasDefaultValue; union at :17789) | crates/tsr-checker/src/destructure.rs:996 destructuring_property_lookup (miss -> intrinsics.error at :1051) | 4 | 4 | 16 |
| 43 | `INSTANTIATION-EXPRESSION-TYPE` | checkExpressionWithTypeArguments/getInstantiationExpressionType (instantiate a value's generic signatures with explicit type args) is unported, both for expression `f<number>` and type query `typeof f<T>`. | internal/checker/checker.go:10660 getInstantiationExpressionType (entry :10637 checkExpressionWithTypeArguments; typeof path :24102 getTypeFromTypeQueryNode) | crates/tsr-checker/src/declared.rs:1220 get_type_from_type_query_node (returns error on type_arguments at :1222) and crates/tsr-checker/src/expressions.rs:606 c | 4 | 4 | 14 |
| 44 | `EXPORT-ASSIGNMENT-ALIAS-LIKE-EXPRESSION` | getTargetOfExportAssignment → getTargetOfAliasLikeExpression resolves any ExpressionIsAlias expression (identifier, entity-name property access, class expression); TSR's export_assignment_target and declaration_of_alias_ | internal/checker/checker.go:14996 getTargetOfAliasLikeExpression (from 14976 getTargetOfExportAssignment) | crates/tsr-checker/src/symbols.rs:1546 export_assignment_target (+1344 declaration_of_alias_symbol ExportAssignment predicate) | 4 | 4 | 13 |
| 45 | `JSDOC-SETTER-PARAM-ACCESSOR-TYPE` | getTypeOfAccessors/getWriteTypeOfAccessors read the setter's parameter annotation, which in JS is the reparsed JSDoc @param type; TSR's accessor_annotation reads only the written r#type of the setter parameter. | internal/checker/checker.go:20106 getAnnotatedAccessorTypeNode (SetAccessor arm -> getEffectiveSetAccessorTypeAnnotationNode; called from getTypeOfAccessors :18 | crates/tsr-checker/src/symbols.rs:375 accessor_annotation | 4 | 4 | 9 |
| 46 | `SPREAD-PROPERTY-ANNOTATION-REUSE` | Spread copies property symbols that keep their original PropertySignature declarations, so printing the spread result goes through serializeTypeForDeclaration on those declarations and reuses the written annotation (an o | internal/checker/nodebuilderimpl.go:2486 addPropertyToElementList -> :2181 serializeTypeForDeclaration (:2249 isOptionalAnnotated strip) | crates/tsr-checker/src/spreads.rs:274 spread_properties (printed_type at :331, optional at :347) | 4 | 4 | 7 |
| 47 | `TYPELIT-INDEX-INFO-KEY-DEDUPE` | getIndexInfosOfIndexSymbol adds an index info for a key type only if findIndexInfo finds none yet, so repeated `[x: number]` signatures yield one; TSR's build_type_literal pushes one rendered index member (and IndexInfo) | internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (findIndexInfo guard :19655) | crates/tsr-checker/src/declared.rs:1935 build_type_literal (index arm :2188-2190: `typed_indexes.extend` / `indexes.push(rendered)`) | 4 | 4 | 6 |
| 48 | `TYPEREF-ARITY-ERRORTYPE-COMPOSES` | getTypeFromClassOrInterfaceReference returns errorType for a generic referenced with the wrong arity, and that errorType is an ordinary type that composes: `IFoo[]` is `any[]`, `C<I>` is `C<any>`. TSR uses the same intri | internal/checker/checker.go:23169 getTypeFromClassOrInterfaceReference (errorType at :23197; composed by createTypeReference / :24115 getTypeFromArrayOrTupleTyp | crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference (arity `return error` :4062; per-argument `if resolved == error { return error }` :4071) | 4 | 4 | 5 |
| 49 | `NODEBUILDER-GET-REDUCED-TYPE` | typeToTypeNode calls getReducedType before printing (unions drop never-reduced intersections; intersections with conflicting discriminants print never); TSR's printer never reduces. | internal/checker/nodebuilderimpl.go:3229 typeToTypeNode -> checker.go:21819 getReducedType | crates/tsr-checker/src/checker.rs:1859 type_to_string_at (reuse flow.rs:6409 intersection_has_never_discriminant) | 7 | 3 | 20 |
| 50 | `BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS` | For an object binding element with a computed property name, getBindingElementTypeFromParentType indexes the parent with getIndexedAccessTypeEx(parent, getLiteralTypeFromPropertyName(name)) (literal/unique-symbol keys se | internal/checker/checker.go:17707 getBindingElementTypeFromParentType (object arm :17740-17742; rest :17737 -> :17792 getRestType; :26769 getLiteralTypeFromProp | crates/tsr-checker/src/destructure.rs:170 get_type_for_binding_element_impl (computed_key arm -> get_applicable_index_info) and destructure.rs:827 object_rest_t | 5 | 3 | 23 |
| 51 | `OBJECT-LITERAL-PSEUDOTYPE-REUSE` | Object-literal property types are serialized via serializeTypeForDeclaration -> pseudochecker (GetTypeOfDeclaration/typeFromExpression; GetTypeOfAccessor). An `as T` / `<T>` assertion yields the asserted type node, `'x'  | internal/pseudochecker/lookup.go:510 typeFromTypeAssertion (from lookup.go:262 typeFromExpression; accessors lookup.go:26 GetTypeOfAccessor), consumed by intern | crates/tsr-checker/src/objects.rs:867 check_object_literal_members (property `printed` at :1760) | 5 | 3 | 18 |
| 52 | `CLASSIFY-PROPERTY-NAME` | Member names are produced by getPropertyNameNodeForSymbol -> classifyPropertyName. A METHOD named `new` becomes a string literal `"new"`, and a name that is not identifier text, including the parser-recovery empty name,  | internal/checker/nodebuilderimpl.go:2384 classifyPropertyName (via :2426 getPropertyNameNodeForSymbol / :2394 createPropertyNameNodeForIdentifierOrLiteral) | crates/tsr-checker/src/objects.rs:2601 written_property_name (plus per-site name arms: declared.rs:2028 type-literal method signature, objects.rs:1169 object-li | 5 | 3 | 8 |
| 53 | `ORIGIN-SLICE-GATE` | getUnionTypeWorker creates a denormalized origin for any union built from named unions (alias/origin) without overlap; TSR build_origin_union's §53 slice gate answers error unless every entry is an enum/alias union or a  | internal/checker/checker.go:25653 getUnionTypeWorker (addNamedUnions/origin :25705-25728) | crates/tsr-checker/src/unions.rs:885 build_origin_union (gate :1001-1046) | 4 | 3 | 24 |
| 54 | `BINDING-PATTERN-IMPLIED-TYPE` | getTypeForVariableLikeDeclaration's final binding-pattern arm returns getTypeFromBindingPattern for ANY declaration with a pattern name and no annotation/initializer (ambient vars, uninitialized vars, rest parameters, pa | internal/checker/checker.go:16790 getTypeForVariableLikeDeclaration (IsBindingPattern arm -> :17904 getTypeFromBindingPattern) | crates/tsr-checker/src/destructure.rs:396 get_type_for_binding_element_parent (no-source VariableDeclaration -> error at :559; parameter container gate :459) an | 4 | 3 | 16 |
| 55 | `CHOOSE-OVERLOAD-CONTEXT-SENSITIVE-ARG-RETENTION` | Context-sensitive arguments are checked once under the first candidate/pass in chooseOverload (assignContextualParameterTypes sets parameter links once; arrow return types cached) and those types feed later candidates/pa | internal/checker/checker.go:10349 assignContextualParameterTypes (with :9025 chooseOverload subtype->assignable passes from :8843 resolveCall) | crates/tsr-checker/src/calls.rs:2480 clean_candidate_prefix_len (excludes generic candidates from subtype pass) + calls.rs:2597 transcribed_generic_set_walk_wor | 4 | 3 | 12 |
| 56 | `CONTEXTUAL-OBJLIT-ELEMENT-INDEX-FALLBACK` | getContextualTypeForObjectLiteralElement falls back, for any named element (including non-bindable computed names), to the contextual type's applicable index info for getLiteralTypeFromPropertyName(name); TSR returns Non | internal/checker/checker.go:29920 getContextualTypeForObjectLiteralElement (dynamic-name arm :29934, index fallback :29946-29955) | crates/tsr-checker/src/contextual.rs:2143 contextual_type_for_object_literal_named_element (`self.late_bound_symbol_member_name(name)?.0` at :2163) | 3 | 3 | 36 |
| 57 | `CHOOSE-OVERLOAD-WRITTEN-TYPE-ARGUMENTS` | chooseOverload's hasCorrectTypeArgumentArity filter + instantiation of the candidate with explicit type arguments (getTypeArgumentsFromNodes) is missing on the overloaded call and tagged-template roads; TSR's multi-candi | internal/checker/checker.go:9214 hasCorrectTypeArgumentArity (used in :9025 chooseOverload) | crates/tsr-checker/src/calls.rs:1720 choose_overload (has_type_arguments excluded) + calls.rs:1162 check_tagged_template_expression + contextual.rs:2446 context | 3 | 3 | 14 |
| 58 | `LATE-BIND-INDEX-SIGNATURE-CLASS-INTERFACE` | Class/interface members whose computed name is an entity-name expression of non-literal string/number/symbol type are late-bound into the __index symbol (getResolvedMembersOrExportsOfSymbol -> lateBindIndexSignature) and | internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (hasLateBindableIndexSignature arm :19662; aggregation :19700-19716 via getObjectLiteralIndexInfo : | crates/tsr-checker/src/index_signatures.rs:375 index_infos_of_symbol | 3 | 3 | 9 |
| 59 | `OBJLIT-PROPERTIES-TABLE-ESCAPED-NAME-KEY` | checkObjectLiteral keys propertiesTable by the member symbol's escaped name (binder getPropertyNameForPropertyNameNode merges `26`, `0b11010`, `"26"`; late-bound computed names use getPropertyNameFromType, so `"1"` and ` | internal/checker/checker.go:13144 checkObjectLiteral (propertiesTable[member.Name] = member at :13331) | crates/tsr-checker/src/objects.rs:2445 upsert_member (called from check_object_literal at :1839 with the printed `name`) | 3 | 3 | 8 |
| 60 | `SUPER-CALL-CONSTRUCTOR-ONLY` | checkSuperExpression's isLegalUsageOfSuperExpression requires a super CALL's container to be a Constructor; TSR accepts super() in any class member (property initializer, method, accessor, static property) and answers ty | internal/checker/checker.go:7866 checkSuperExpression.isLegalUsageOfSuperExpression (isCallExpression arm :7867-7870; func :7854) | crates/tsr-checker/src/expressions.rs:2007 check_super_expression | 3 | 3 | 6 |
| 61 | `TYPELIT-DUPLICATE-PROPERTY-SYMBOL-MERGE` | The binder's declareSymbol merges same-named property signatures of a type literal into one member symbol, so resolveAnonymousTypeMembers yields one property; TSR's build_type_literal renders one Member::Property per dec | internal/binder/binder.go:152 declareSymbolEx (same-name merge) -> internal/checker/checker.go:16124 getMembersOfSymbol | crates/tsr-checker/src/declared.rs:1935 build_type_literal (property push at :2421-2435) | 3 | 3 | 5 |
| 62 | `INFER-NO-CANDIDATE-GUARD` | getInferredType's no-candidate arm (default type -> unknown, then constraint) is blocked by TSR's 'structural_source_supplied' refusal: when an argument mentions the type parameter's position but inference collected no c | internal/checker/inference.go:1317 getInferredType | crates/tsr-checker/src/inference.rs:560 check_generic_call_worker (no-candidate decline L1371-1416) | 5 | 2 | 26 |
| 63 | `CHOOSE-OVERLOAD-GENERIC-WALK` | chooseOverload's per-candidate loop for sets containing generic candidates (infer type args with this candidate, inferential arg check, applicability; fall through on failure) is replaced by transcribed_generic_set_walk  | internal/checker/checker.go:9025 chooseOverload | crates/tsr-checker/src/calls.rs:2546 transcribed_generic_set_walk / :2597 transcribed_generic_set_walk_worker (gated at calls.rs:1734 choose_overload) | 5 | 2 | 22 |
| 64 | `IMPORT-TYPE-NODE-TYPEOF` | getTypeFromImportTypeNode's value-meaning arm (`typeof import("m")[.q]`, qualifier resolved through property-of-type, plus instantiation-expression type arguments via resolveImportSymbolType) is not ported; TSR returns e | internal/checker/checker.go:24575 getTypeFromImportTypeNode (IsTypeOf: :24608-24609 property lookup, :24660-24662 getInstantiationExpressionType) | crates/tsr-checker/src/declared.rs:4600 get_type_from_import_type_node (`if node.is_type_of` early error at :4602) | 5 | 2 | 19 |
| 65 | `EXTERNAL-MODULE-MEMBER-EXPORT-EQUALS` | getExternalModuleMember on an export= module takes symbolFromVariable = getPropertyOfType(typeof target, name) and combines it with symbolFromModule; TSR declines every value member whose type is not a site-independent s | internal/checker/checker.go:14667 getExternalModuleMember (export= branch 14698-14732) | crates/tsr-checker/src/symbols.rs:1737 get_external_module_member (gate 1795-1818, 1834) | 4 | 2 | 25 |
| 66 | `CONTEXTUAL-RETURN-GENERATOR-FILTER` | getContextualReturnType's generator arm (filter a union contextual return type to constituents a generator can be assigned to) and getContextualIterationType (iteration types of that filtered type) are unported; TSR read | internal/checker/checker.go:29665 getContextualReturnType (generator filter :29678-29682) ; :29656 getContextualIterationType ; :29719 getContextualTypeForYield | crates/tsr-checker/src/contextual.rs:1571 contextual_generator_iteration_type (callers: crates/tsr-checker/src/expressions.rs:3656-3663, crates/tsr-checker/src/ | 4 | 2 | 20 |
| 67 | `RESOLVE-ALIAS-INDIRECTION` | resolveAlias applies resolveIndirectionAlias when the immediate target is itself a non-local pure alias, so alias targets are final; TSR's resolve_alias is one hop, and the type-reference road and the writer's type-only  | internal/checker/checker.go:16266 resolveAlias (16280 IsNonLocalAlias → 16293 resolveIndirectionAlias); 24094 getDeclaredTypeOfAlias | crates/tsr-checker/src/symbols.rs:943 resolve_alias (consumers: declared.rs:1565 alias road TYPE test) | 4 | 2 | 18 |
| 68 | `YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE` | checkAndAggregateYieldOperandTypes adds getContextualType(yield) to the generator's NEXT aggregate for every non-star yield (nil adds nothing; the contextual-iteration fallback then fills the slot); TSR instead keeps a p | internal/checker/checker.go:20322 checkAndAggregateYieldOperandTypes (nextType = getContextualType(yieldExpr) :20338-20340) via :20126 getReturnTypeFromBody | crates/tsr-checker/src/signatures.rs:2497 return_type_from_body (decline predicate 2728-2868, recorded_next arms 2869-2971: `if contextual && !recorded_next { r | 4 | 2 | 6 |
| 69 | `INTERFACE-BASE-FROM-TYPE-NODE` | resolveBaseTypesOfInterface takes getTypeFromTypeNode of each heritage entry (alias resolved and instantiated: alias-to-object-literal, alias-to-intersection, Record<..> mapped type) and accepts any isValidBaseType objec | internal/checker/checker.go:19498 resolveBaseTypesOfInterface (+ :19537 isValidBaseType) | crates/tsr-checker/src/members.rs:3182 base_symbols_of_ex / base_symbol_of_heritage_entry :3211 | 3 | 2 | 30 |
| 70 | `JSDOC-ON-FUNCTION-EXPRESSION-HOSTING` | The parser attaches leading JSDoc to arrow functions/function expressions (parseParenthesizedArrowFunctionExpression/parseSimpleArrowFunctionExpression/parseFunctionExpression carry jsdoc and reparseTags runs on them), s | internal/parser/parser.go:4341 parseParenthesizedArrowFunctionExpression (+ :4541 parseSimpleArrowFunctionExpression, :5677 parseFunctionExpression) -> reparser | crates/tsr-parser/src/expression.rs:1647 try_parse_arrow_function (and :2217 parse_function_expression) | 3 | 2 | 18 |
| 71 | `GENERIC-ARG-NIL-CONTEXTUAL-SIGNATURE` | For a context-sensitive arg whose parameter type is a type parameter (apparent type with zero or several call signatures), getContextualSignature/getContextualCallSignature return nil so the arrow is typed non-contextual | internal/checker/checker.go:10305 getContextualCallSignature (nil unless exactly one applicable sig) / :10264 getContextualSignature | crates/tsr-checker/src/signatures.rs:6214 get_type_of_function_expression (ContextualSignature::Absent => !single_generic_argument_context) | 3 | 2 | 18 |
| 72 | `LATE-BOUND-SIGNATURES-OF-SYMBOL-IMPL-SKIP` | Late-bound method overloads merge into one late symbol whose signatures come from getSignaturesOfSymbol, which skips an implementation body immediately following a same-kind declaration; TSR's §383 sibling walk collects  | internal/checker/checker.go:19806 getSignaturesOfSymbol (implementation skip :19818-19823) over the symbol built by :16005 lateBindMember | crates/tsr-checker/src/symbols.rs:2732 get_type_of_func_class_enum_module_worker (`late_bound_overloads` block :2799-2857) | 3 | 2 | 13 |
| 73 | `GLOBALTHIS-SYMBOL-IN-GLOBALS` | NewChecker creates globalThisSymbol (Module, exports = globals) and inserts it into globals; TSR has no such symbol, only an identifier-expression special case, so name resolution (`import mod = globalThis`, `export { gl | internal/checker/checker.go:962 NewChecker globalThisSymbol setup (962-964) | crates/tsr-checker/src/expressions.rs:894 identifier `globalThis` special case (global_this_type); binder globals lack the symbol | 3 | 2 | 8 |
| 74 | `RESOLVE-NAME-EXPORTED-IMPORT-EQUALS` | NameResolver's module-exports lookup uses getSymbol with meaning&ModuleMember, which accepts an ALIAS export whose target carries the meaning; TSR's binder exports arm admits exported import-equals aliases only for Quali | internal/binder/nameresolver.go:138 Resolve (module exports lookup, meaning&SymbolFlagsModuleMember) | crates/tsr-binder/src/lib.rs:802 resolve_name_excluding_with_export_alias exports arm (ImportEqualsDeclaration admission 828-843) | 3 | 2 | 7 |
| 75 | `AMBIENT-MODULE-SPECIFIER-MULTIDECL` | getSpecifierForModuleSymbol's ambient half spells any module symbol with no SourceFile declaration as its stripped name; TSR's type_to_string_at_worker only spells `typeof import("x")` when the ambient module has exactly | internal/checker/nodebuilderimpl.go:1249 getSpecifierForModuleSymbol (ambient arm :1260-1263) | crates/tsr-checker/src/checker.rs:1869 type_to_string_at_worker (gate `if let [declaration] = declarations` at :2150) | 3 | 2 | 6 |
| 76 | `TYPE-PARAMETER-CONSTRAINT-NODE-REUSE` | typeParameterToDeclaration prints the constraint by reusing the written constraint node whenever getTypeFromTypeNode(constraintNode) == constraint (typeToTypeNodeHelperWithPossibleReusableTypeNode). TSR's written_constra | internal/checker/nodebuilderimpl.go:1611 typeParameterToDeclaration (-> :1597 typeToTypeNodeHelperWithPossibleReusableTypeNode) | crates/tsr-checker/src/signatures.rs:5754 type_parameter_of (written_constraint at :5800) | 3 | 2 | 5 |
| 77 | `UNTYPED-CALL-FUNCTION-TYPED-CALLEE` | isUntypedFunctionCall's third disjunct - a non-union, non-never callee with no call/construct signatures that is assignable to globalFunctionType (e.g. typed `Function`) is an untyped call -> any; TSR ports only the any  | internal/checker/checker.go:9936 isUntypedFunctionCall (numCallSignatures==0 && numConstructSignatures==0 && !union && !never && isTypeAssignableTo(funcType, gl | crates/tsr-checker/src/calls.rs:2113 is_untyped_call_target (doc at calls.rs:2091-2095 lists this arm as unported; reuse the Function-assignability code at call | 3 | 2 | 4 |
| 78 | `EXPANDO-ASSIGNMENT-LATE-BIND-MEMBER` | Expando element assignments `foo[c] = v` with a dynamic name are bound as __computed and recorded on foo's __assignmentDeclaration export, then late-bound into foo's resolved exports by lateBindMember; TSR's binder drops | internal/checker/checker.go:15962 getResolvedMembersOrExportsOfSymbol (assignment-declaration arm) -> :16005 lateBindMember; binder internal/binder/binder.go:10 | crates/tsr-checker/src/callable_expandos.rs:16 callable_export_properties (+ crates/tsr-binder/src/binder.rs:3535 bind_deferred_expando_assignment, `let name =  | 2 | 2 | 36 |
| 79 | `SYMBOL-NAME-AS-WRITTEN-SOURCE-TEXT` | getNameOfSymbolAsWritten prints a symbol name as scanner.DeclarationNameToString(declaration name), i.e. the declaration's SOURCE text, which keeps unicode escapes like `C\u0032`. TSR prints the cooked identifier text /  | internal/checker/nodebuilderimpl.go:973 getNameOfSymbolAsWritten (:1002 DeclarationNameToString) | crates/tsr-checker/src/declared.rs:6291 new_named_type (declared_name from identifier.text at :6327); value side crates/tsr-checker/src/checker.rs:2255 value_sy | 2 | 2 | 19 |
| 80 | `SYMBOLCONSTRUCTOR-UNIQUE-SYMBOL-COMPAT` | widenTypeForVariableLikeDeclaration turns an ESSymbol-typed member declared in the global SymbolConstructor into a unique symbol (getESSymbolLikeTypeForNode); TSR keeps `symbol`. | internal/checker/checker.go:18246 widenTypeForVariableLikeDeclaration (isGlobalSymbolConstructor arm; func :18242, helper :18236, :22982 getESSymbolLikeTypeForN | crates/tsr-checker/src/symbols.rs:4247 get_widened_type_for_variable_like_declaration | 2 | 2 | 16 |
| 81 | `JSDOC-TYPE-TAG-HOSTING` | reparseHosted moves a JSDoc @type onto the Type of any hostable declaration (PropertyDeclaration, ExportAssignment, PropertyAssignment, GetAccessor, VariableDeclaration), so it is both the declared type and the contextua | internal/parser/reparser.go:342 reparseHosted (KindJSDocTypeTag arm, PropertyDeclaration/ExportAssignment case :356) | crates/tsr-checker/src/symbols.rs:5693 jsdoc_type_annotation (VariableDeclaration-only gate :5697) + crates/tsr-checker/src/contextual.rs:1084 get_contextual_ty | 2 | 2 | 11 |
| 82 | `JSDOC-BARE-TYPEDEF-TYPE-LITERAL` | A `@typedef Name` with no {type} (or {Object}) followed by @property tags is reparsed into a type alias whose body is a JSDocTypeLiteral; TSR's binder only creates the member-owning alias when the typedef has an explicit | internal/parser/reparser.go:240 reparseJSDocTypeLiteral (via reparseUnhosted typedef arm reparser.go:70) | crates/tsr-binder/src/binder.rs:2984 bind_jsdoc_declarations (property_owner requires typedef.type_expression at :3016) + crates/tsr-checker/src/declared.rs:621 | 2 | 2 | 11 |
| 83 | `FOR-AWAIT-OF-BINDING-PARENT` | A binding pattern under `for await` takes its parent type from getTypeForVariableLikeDeclaration's ForOf arm (checkRightHandSideOfForOf, which handles async iteration); TSR's binding-parent copy of that arm explicitly ex | internal/checker/checker.go:16664 getTypeForVariableLikeDeclaration (ForOfStatement arm -> checkRightHandSideOfForOf) via :17695 getTypeForBindingElementParent | crates/tsr-checker/src/destructure.rs:496 get_type_for_binding_element_parent (for-of arm gated `for_of.await_modifier.is_none()` at :500) | 2 | 2 | 10 |
| 84 | `JS-DEFAULT-EXPORT-JSDOC-TYPE` | The `default` export of a JS ExportAssignment carrying a JSDoc @type (hosted or as a parenthesized cast) has the tag's type; TSR deliberately returns error for it because importing files must print the typedef alias modu | internal/parser/reparser.go:342 reparseHosted (ExportAssignment / ParenthesizedExpression -> makeNewCast :674); typed via getTypeOfVariableOrParameterOrProperty | crates/tsr-checker/src/symbols.rs:4000 ExportAssignment arm of the symbol-type worker (jsdoc_cast_annotation gates at :4008 and :4021) | 2 | 2 | 6 |
| 85 | `JSDOC-TYPE-GRAMMAR` | parseJSDocType/parseNonArrayType parse JSDoc-only type syntax (`...T` variadic, `*` all-type, `?T`/`T?` nullable) inside {braces}; TSR's parse_jsdoc_type_expression hands off to the plain TS parse_type, so these nodes ar | internal/parser/parser.go:2858 parseJSDocType (variadic :2866; JSDocAllType :2842; prefix nullable :2855; postfix :2728) | crates/tsr-parser/src/jsdoc.rs:762 parse_jsdoc_type_expression (+ types.rs parse_non_array_type/parse_postfix_type in JSDoc context) | 2 | 2 | 6 |
| 86 | `IMPORT-CALL-ARGUMENT-CONTEXT` | getContextualTypeForArgumentAtIndex's import-call arm (argument 0 of import(...) is contextually string, 1 is ImportCallOptions) is unported, so a `yield` inside import(...) has no contextual next type. | internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex (IsImportCall arm :29773-29781) | crates/tsr-checker/src/contextual.rs:2348 contextual_type_for_argument | 2 | 2 | 2 |
| 87 | `IMPORT-EQUALS-ALIAS-TYPEREF-TARGET` | An UNQUALIFIED type reference through an import-equals alias (`var v: a`, `Foo<number>` with `import a = require(...)` of an `export = class C<T>`) goes through tsgo's resolveEntityName alias loop and then getTypeReferen | internal/checker/checker.go:23146 getTypeReferenceType (reached after resolveEntityName's alias loop at :15821; arity error at :23197) | crates/tsr-checker/src/declared.rs:1465 get_type_from_type_reference §157 ImportEquals alias mint (:1465-1505; argument-bearing references fall to :1620 with th | 2 | 2 | 2 |
| 88 | `JS-RETURN-TYPE-UNION-OF-RETURNS` | getReturnTypeFromBody unions all return expression types (subtype reduction) regardless of file kind; TSR declines (None) whenever a JS function has 2+ distinct return types. | internal/checker/checker.go:20126 getReturnTypeFromBody (aggregation via :20259 checkAndAggregateReturnExpressionTypes) | crates/tsr-checker/src/signatures.rs:3470 return_type_from_body (`many =>` arm's in_js_file decline; same decline at :3057, :3090, :3310) | 2 | 2 | 2 |
| 89 | `CONDITIONAL-DEFERRAL-GATE` | getConditionalType defers only when isDeferredType(checkType/extendsType) (isGenericType: type variables, index types, generic mapped/tuple), and for a generic check still resolves to the false branch when permissive ins | internal/checker/checker.go:24300 getConditionalType (checkTypeDeferred :24328, permissive false arm :24377); :24475 isDeferredType | crates/tsr-checker/src/declared.rs:6815 evaluate_conditional_node (L6848-6852 mentions_registered_type_parameter gate) and :7252 evaluate_conditional_inference | 6 | 1 | 14 |
| 90 | `CONDITIONAL-TYPE-ROOT-ALIAS` | A reference to a conditional-bodied alias outside an alias declaration has no new alias, so getConditionalType's deferred arm labels the result with the conditional ROOT's alias instantiated through the mapper (Parameter | internal/checker/checker.go:24300 getConditionalType (deferred arm :24436 result.alias = c.instantiateTypeAlias(root.alias, mapper)); reached via :23641 getType | crates/tsr-checker/src/declared.rs:4167 get_instantiated_type_reference (conditional-alias arm gated by in_alias_declared_position; otherwise falls to create_ty | 5 | 1 | 13 |
| 91 | `INSTANTIATE-MAPPED-TYPE-ALIAS-DROP` | instantiateMappedType's homomorphic arm (mapTypeWithAlias) keeps the enclosing alias only for a UNION type-variable image; for a non-union object/array/generic image it calls instantiateAnonymousType(t, mapper, nil) so t | internal/checker/checker.go:22535 instantiateMappedType (:22565 instantiateAnonymousType(..., nil); :22570 mapTypeWithAlias -> :25554) | crates/tsr-checker/src/mapped.rs:766 instantiate_mapped_type (+ alias naming in crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference / :4349  | 5 | 1 | 11 |
| 92 | `FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS` | tsgo assigns contextual parameter types verbatim even when they mention outer (non-adopted) type parameters; TSR's get_type_of_function_expression 'grounded' gate returns error whenever a materialised contextual param ty | internal/checker/checker.go:10349 assignContextualParameterTypes (via :10152 contextuallyCheckFunctionExpressionOrObjectLiteralMethod) | crates/tsr-checker/src/signatures.rs:6201 get_type_of_function_expression (grounded / mentions_any_type_parameter gate) | 4 | 1 | 6 |
| 93 | `WIDEN-NULLABLE-TO-ANY` | getWidenedTypeWithContext's RequiresWidening arm turns Nullable (widening) types into any, recursively through array elements and function return types (non-strict `[null,null]`->any[], `() => null`->() => any); TSR wide | internal/checker/checker.go:18359 getWidenedTypeWithContext (Nullable->anyType arm at :18368) | crates/tsr-checker/src/widening.rs:95 widen_type_with_context (and :157 widen_array_members) | 3 | 1 | 19 |
| 94 | `GENERATOR-ANNOTATION-ITERATION-TYPES` | checkYieldExpression's non-star arm reads the NEXT type from the annotated return type via union filter (checkGeneratorInstantiationAssignabilityToReturnType) + getIterationTypesOfGeneratorFunctionReturnType (iterable th | internal/checker/checker.go:10952 checkYieldExpression (returnType filter + :6224 getIterationTypesOfGeneratorFunctionReturnType -> :6265 getIterationTypesOfIte | crates/tsr-checker/src/declared.rs:191 next_type_of_annotated_generator (callers crates/tsr-checker/src/expressions.rs:3564) | 3 | 1 | 6 |
| 95 | `SHADOWED-TYPEPARAM-RENAME` | typeParameterToName with GenerateNamesForShadowedTypeParams renames a shadowing type parameter once (cached per type id) and every reference — declaration, constraint, parameter, nested conditional/indexed — uses the cac | internal/checker/nodebuilderimpl.go:1404 typeParameterToName (:1396 typeParameterShadowsOtherTypeParameterInScope) | crates/tsr-checker/src/inference.rs:5583 rename_type_parameters_for_site / :5694 rename_own_type_parameters_for_print; printing.rs:126 allocate_type_parameter_n | 3 | 1 | 5 |
| 96 | `DECLARED-TYPE-OF-ALIAS` | tryGetDeclaredTypeOfSymbol has an Alias arm (getDeclaredTypeOfAlias = declared type of resolveAlias target). TSR's get_declared_type_of_symbol has none and answers error for alias symbols, so the RHS/export arms fall bac | internal/checker/checker.go:23678 tryGetDeclaredTypeOfSymbol (Alias case) -> checker.go:24094 getDeclaredTypeOfAlias | crates/tsr-checker/src/declared.rs:5456 get_declared_type_of_symbol | 3 | 1 | 3 |
| 97 | `PREFIX-NOT-TYPE-FACTS` | `!x` result comes from getTypeFacts(operand, Truthy\|Falsy) which handles instantiable types via their constraint/base; TSR's hand-written negated_truthiness_type table returns error for type parameters / indexed access. | internal/checker/checker.go:10887 checkPrefixUnaryExpression ExclamationToken arm -> :30974 getTypeFacts | crates/tsr-checker/src/expressions.rs:1463 negated_truthiness_type | 2 | 1 | 3 |
| 98 | `CONDITIONAL-GENERIC-SIGNATURE-EXTENDS` | The Equals idiom `(<A>() => A extends X ? 1 : 0) extends (<A>() => A extends Y ? 1 : 0)` answers error in TSR even for concrete X/Y: the inline conditional in the generic signature's return is not instantiable and generi | internal/checker/checker.go:24300 getConditionalType (assignability of generic signatures via compareSignaturesRelated)  | crates/tsr-checker/src/declared.rs:6815 evaluate_conditional_node | 2 | 1 | 2 |
| 99 | `JS-EXPANDO-EMPTY-OBJECT-LITERAL` | checkObjectLiteral returns an anonymous type over node.Symbol().Exports for an empty JS object literal whose symbol has expando members; TSR's check_object_literal has no such arm, so `const A = {}; A.x = ...` stays `{}` | internal/checker/checker.go:13146 checkObjectLiteral (empty-literal-with-exports arm) | crates/tsr-checker/src/objects.rs:857 check_object_literal | 1 | 1 | 26 |
| 100 | `MAPPED-MEMBERS-LOWER-BOUND-KEY` | resolveMappedTypeMembers on a generic key constraint (Extract<keyof T,'b'>, keyof T\|'c', keyof (T&U)) enumerates getLowerBoundOfKeyType; TSR mapped_member_keys returns None for any generic key. | internal/checker/checker.go:21021 getLowerBoundOfKeyType (called from resolveMappedTypeMembers :20894) | crates/tsr-checker/src/mapped.rs:481 mapped_member_keys (L556/L583 signature_parameter_type_is_generic → None) | 1 | 1 | 16 |
| 101 | `KEYOF-RESOLVED-OPERAND-INDEX-TYPE` | getTypeFromTypeOperatorNode is getIndexType(getTypeFromTypeNode(operand)) for ANY operand, but TSR's keyof node arm only resolves syntactic union/intersection operands, generic-alias references and bare type parameters,  | internal/checker/checker.go:22960 getTypeFromTypeOperatorNode -> :26680 getIndexType (union arm = intersection of constituent key sets) | crates/tsr-checker/src/declared.rs:621 get_type_from_type_node keyof arm (fallthrough `None => error` at :737); crates/tsr-checker/src/declared.rs:7555 resolved | 1 | 1 | 11 |
| 102 | `JSDOC-SCAN-UNICODE-ESCAPE` | ScanJSDocToken decodes `\uXXXX`/`\u{X}` escapes in JSDoc identifiers so `@param {number} a\u0061` names `aa`; TSR's scan_jsdoc_token has no backslash arm, so the name never matches. | internal/scanner/scanner.go:1490 ScanJSDocToken (`case '\\'` arm; func at :1418) | crates/tsr-scanner/src/jsdoc.rs:75 scan_jsdoc_token | 1 | 1 | 8 |
| 103 | `INSTANTIATE-ANONYMOUS-TYPE-CACHE-FIRST` | instantiateAnonymousType creates and caches the instantiated object type BEFORE its members are resolved (members are instantiated lazily), so a self-referential object literal type terminates; TSR instantiate_anonymous_ | internal/checker/checker.go:22304 getObjectTypeInstantiation / :22458 instantiateAnonymousType | crates/tsr-checker/src/inference.rs:5362 instantiate_anonymous_properties | 1 | 1 | 7 |
| 104 | `EXPORT-ASSIGNMENT-CLASS-EXPRESSION-TARGET` | getTargetOfAliasLikeExpression resolves `export = <class expression>` to the class expression's symbol (checkExpressionCached(expr).symbol); TSR's export_assignment_target handles only an Identifier, so `import Chunk = r | internal/checker/checker.go:14996 getTargetOfAliasLikeExpression (:14997 IsClassExpression arm), via :14976 getTargetOfExportAssignment | crates/tsr-checker/src/symbols.rs:1546 export_assignment_target | 1 | 1 | 5 |
| 105 | `STATIC-SIDE-FROM-BASE-CONSTRUCTOR-TYPE` | A class constructor type inherits static members via resolveAnonymousTypeMembers adding getPropertiesOfType(getBaseConstructorTypeOfClass(classType)); TSR's static lookup walks base symbols with base_symbols_of (refuse_t | internal/checker/checker.go:20650 resolveAnonymousTypeMembers (:20686-20690 getBaseConstructorTypeOfClass + addInheritedMembers) | crates/tsr-checker/src/members.rs:2504 static_property_of_bases (base_symbols_of at :2510; also :3004 collect_static_property_names) | 1 | 1 | 4 |
| 106 | `STRICT-SUBTYPE-FRESH-EMPTY-TARGET` | Under subtype/strictSubtype relations a non-empty source is not related to a fresh empty object literal target; TSR treats `BarProps & object` as a strict subtype of fresh `{}` and keeps `{}`. | internal/checker/relater.go:3853 structuredTypeRelatedToWorker (fresh-empty-target arm; func at :3261) | crates/tsr-checker/src/relater.rs:618 relate_ternary | 1 | 1 | 4 |
| 107 | `IDENTIFIER-EXPORT-SYMBOL-OF-VALUE` | checkIdentifier maps the resolved local through getExportSymbolOfValueSymbolIfExported before typing; TSR types the local directly, and get_type_of_symbol's ALIAS arm (87) wins over the EXPORT_VALUE arm (92) for an Alias | internal/checker/checker.go:14383 getExportSymbolOfValueSymbolIfExported | crates/tsr-checker/src/symbols.rs:87 get_type_of_symbol (ALIAS arm before EXPORT_VALUE marker arm 92) | 1 | 1 | 3 |
| 108 | `CONDITIONAL-TAIL-RECURSION` | getConditionalType's tail-recursion loop (up to 1000 iterations without consuming instantiation depth) is not ported; TSR recurses and stops at ~60 levels. | internal/checker/checker.go:24300 getConditionalType (tailCount loop :24303-24310, getTailRecursionRoot :24418) | crates/tsr-checker/src/declared.rs:6429 evaluate_conditional_alias / :6815 evaluate_conditional_node | 1 | 1 | 3 |
| 109 | `JSDOC-PARAM-MATCH-BINDING-PATTERN` | findMatchingParameter matches a @param tag to a binding-pattern parameter by tag index; TSR's jsdoc_parameter_annotation returns None for any non-Identifier parameter name. | internal/parser/reparser.go:621 findMatchingParameter | crates/tsr-checker/src/symbols.rs:5620 jsdoc_parameter_annotation (Identifier-only at :5630) and signatures.rs:1395 param_types name match | 1 | 1 | 3 |
| 110 | `UNRESOLVED-IMPORT-ALIAS-VALUE-ERRORTYPE` | getTypeOfAlias for an import whose module is unresolvable yields errorType. With no error baseline (`// @ts-ignore` on the import) the types writer prints IsTypeAny types by intrinsic name `error`; TSR answers anyType fo | internal/checker/checker.go:18598 getTypeOfAlias | crates/tsr-checker/src/symbols.rs:409 get_type_of_alias (unfindable ES-import arm :465-479 inserts intrinsics.any) | 1 | 1 | 2 |
| 111 | `EXTERNAL-MODULE-MEMBER-SHORTHAND-AMBIENT` | getExternalModuleMember returns the module symbol itself for a shorthand ambient module (`declare module "jquery";`); TSR lacks this arm and only special-cases direct imports from unfindable/shorthand modules elsewhere,  | internal/checker/checker.go:14693 getExternalModuleMember isShorthandAmbientModuleSymbol arm | crates/tsr-checker/src/symbols.rs:1737 get_external_module_member | 1 | 1 | 2 |
| 112 | `MAPPED-KEYOF-MEMBER-ORDER` | Homomorphic mapped members follow getPropertiesOfType(modifiers) declaration order; TSR's property name enumeration for Number yields a different order. | internal/checker/checker.go:22727 forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType | crates/tsr-checker/src/mapped.rs:481 mapped_member_keys (property_names_of at L529) | 1 | 1 | 2 |
| 113 | `OBJLIT-MISSING-NAME-PROPERTY-MEMBER` | A parser-recovered PropertyAssignment with a missing (empty) name still binds a property symbol named "" and checkObjectLiteral adds it to propertiesTable (`"": any`); TSR's check_object_literal skips every empty-text id | internal/checker/checker.go:13144 checkObjectLiteral (member added at :13331 regardless of empty name) | crates/tsr-checker/src/objects.rs:857 check_object_literal (`PropertyName::Identifier(name) if name.text.is_empty() => continue` at :1511) | 1 | 1 | 2 |
| 114 | `LATE-BIND-MEMBER-OBJLIT-ACCESSOR-MERGE` | lateBindMember also runs for object-literal containers (SymbolFlagsLateBindingContainer includes ObjectLiteral), merging a computed `get`/`set` pair into one late symbol whose getTypeOfAccessors reads the getter; TSR's l | internal/checker/checker.go:16005 lateBindMember (via :15930 getResolvedMembersOrExportsOfSymbol for the object-literal symbol) -> :18511 getTypeOfAccessors | crates/tsr-checker/src/members.rs:2539 late_bound_members_of (no ObjectLiteralExpression arm in the match at :2553), consumed by crates/tsr-checker/src/symbols. | 1 | 1 | 2 |
| 115 | `SETTER-PARAM-FROM-GETTER-OBJLIT` | getTypeForVariableLikeDeclaration's set-accessor-parameter arm takes the paired getter's return type for any container; TSR's paired_get_accessor only scans class members, so a JS object-literal setter param is any. | internal/checker/checker.go:16718 getTypeForVariableLikeDeclaration (set-accessor parameter arm) | crates/tsr-checker/src/symbols.rs:5586 paired_get_accessor | 1 | 1 | 1 |
| 116 | `IMPORTED-ALIAS-REFERENCE-WRITTEN-ARGS` | getTypeFromTypeAliasReference's import/export arm is not ported: a generic alias referenced through an import specifier (outside an alias body) gets alias = the resolved target with aliasTypeArguments = the WRITTEN argum | internal/checker/checker.go:23617 getTypeFromTypeAliasReference (IsTypeReferenceType import/export alias arm, 23617-23627) | crates/tsr-checker/src/declared.rs:1397 get_type_from_type_reference (alias_road branch calling get_instantiated_type_reference at :1598) | 1 | 1 | 1 |
| 117 | `IMPORT-EQUALS-RHS-NAMESPACE-LOOKUP` | Identifiers in an import-equals RHS are typed via getSymbolAtLocation → getSymbolOfPartOfRightHandSideOfImportEquals, a namespace-exports lookup; an unresolved segment gives errorType (any). TSR's writer falls through to | internal/checker/checker.go:14474 getSymbolOfPartOfRightHandSideOfImportEquals | crates/tsr-conformance/src/types_producer.rs:1285 import-equals qualified-name leaf rule (falls through when exports lookup misses) | 1 | 1 | 1 |
| 118 | `PRINT-ARRAY-TARGET-DECLARED-TYPE` | The declared (self-referential) type of the global Array/ReadonlyArray interface is printed by typeReferenceToTypeNode as `T[]`/`readonly T[]`; TSR's declared-interface mint prints `Array<T>`. | internal/checker/nodebuilderimpl.go:2977 typeReferenceToTypeNode (globalArrayType/globalReadonlyArrayType arm 2979-2995) | crates/tsr-checker/src/declared.rs:6291 new_named_type (via declared.rs:5949 get_declared_type_of_class_or_interface; the `T[]` spelling rule lives only in decl | 1 | 1 | 1 |
| 119 | `SPECIFIER-PROPERTYNAME-IMMEDIATE-ALIAS` | getSymbolAtLocation on an export specifier's propertyName returns getImmediateAliasedSymbol (the target module's export), so the line types that export even when it is any; TSR's writer treats an any target as 'unresolve | internal/checker/checker.go:2155 getImmediateAliasedSymbol (via getSymbolAtLocation specifier propertyName arm) | crates/tsr-conformance/src/types_producer.rs:703 specifier property-name arm (any/error fallthrough 717-723) | 1 | 1 | 1 |
| 120 | `CREATE-GENERATOR-TYPE-EMPTY-FALLBACK` | createGeneratorType returns emptyObjectType `{}` when neither global Generator nor IterableIterator exists; TSR's `?` on the missing global turns the signature into error. | internal/checker/checker.go:20434 createGeneratorType (fallback :20442-20447) | crates/tsr-checker/src/signatures.rs:2497 return_type_from_body (generator global lookup 3078-3084) | 1 | 1 | 1 |
| 121 | `CONDITIONAL-INLINE-NODE-INSTANTIATION` | getConditionalTypeInstantiation for an anonymous conditional type node (signature return/param, interface member) is only ported for type-parameter constraints; elsewhere TSR returns error, and deferred results are also  | internal/checker/checker.go:22485 getConditionalTypeInstantiation | crates/tsr-checker/src/declared.rs:6728 instantiate_conditional_node (L6759 `if !in_constraint return error`; L6776 deferred-result filter) | 7 | 0 | 40 |
| 122 | `CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES` | Inherited instance members come from resolveObjectTypeMembers merging getBaseTypes(source) (base TYPES, from the extends expression's construct signatures); TSR walks base SYMBOLS via base_symbols_of/base_symbol_of_herit | internal/checker/checker.go:19106 resolveObjectTypeMembers (base merge :19127-19152) over :19167 getBaseTypes | crates/tsr-checker/src/members.rs:3087 get_property_of_declared_symbol and :1675 generic_heritage_member (via :3182 base_symbols_of_ex / :3211 base_symbol_of_he | 6 | 0 | 28 |
| 123 | `SUPER-FROM-BASE-TYPES` | checkSuperExpression answers errorType when getBaseTypes(class) is empty, getBaseConstructorTypeOfClass(class) for super(...)/static, else getTypeWithThisArgument(getBaseTypes[0]); TSR's check_super_expression re-derives | internal/checker/checker.go:7854 checkSuperExpression (base arm :7933-7961) | crates/tsr-checker/src/expressions.rs:2007 check_super_expression (tail :2125-2165) | 4 | 0 | 13 |
| 124 | `PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED` | Parameter type serialization: upstream prints getTypeOfSymbol(param), which includes the optional `\| undefined` under strict. It also adds getOptionalType for required-initialized params (requiresAddingImplicitUndefined) | internal/checker/nodebuilderimpl.go:2181 serializeTypeForDeclaration (:2216-2220 requiresAddingUndefined/getOptionalType; :2249 isOptionalAnnotated equivalence; | crates/tsr-checker/src/signatures.rs:4670 parameter_of (r#type = raw annotation type, :4859) and crates/tsr-checker/src/inference.rs:5831 instantiate_signature  | 4 | 0 | 7 |
| 125 | `REMOVE-SUBTYPES-UNDECIDABLE` | removeSubtypes (UnionReductionSubtype from conditional expressions and \|\|) needs isTypeStrictSubtypeOf over concrete object pairs (tuple vs object literal, unique symbol vs object); TSR union_with_subtype_reduction retur | internal/checker/checker.go:25934 removeSubtypes (callers: :10934 checkConditionalExpression, :12509 checkBinaryLikeExpressionWorker) | crates/tsr-checker/src/unions.rs:1516 union_with_subtype_reduction | 3 | 0 | 13 |
| 126 | `SPECIFIER-FOR-MODULE-SYMBOL` | getSpecifierForModuleSymbol (module specifier generation) is only approximated: TSR spells `import("./stem")` from its stored path without removing declaration extensions and declines for any path with directories (node_ | internal/checker/nodebuilderimpl.go:1249 getSpecifierForModuleSymbol | crates/tsr-checker/src/checker.rs:2806-2857 symbol_chain (file-module specifier arm) | 3 | 0 | 6 |
| 127 | `SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER` | getSymbolChain's export= container handling (getWithAlternativeContainers / getAliasForSymbolInContainer) is not ported: a namespace that is a module's `export =` target is named by the module itself, and its members qua | internal/checker/symbolaccessibility.go:117 getWithAlternativeContainers / :342 getAliasForSymbolInContainer (from nodebuilderimpl.go:1087 getSymbolChain) | crates/tsr-checker/src/checker.rs:2745 symbol_chain | 3 | 0 | 5 |
| 128 | `JSDOC-GATHER-TYPE-PARAMETERS` | Reparser gatherTypeParameters collects type parameters from ALL @template tags of a comment and puts the tag's {constraint} on the first parameter, and hosts @template on class declarations; TSR takes only the first @tem | internal/parser/reparser.go:293 gatherTypeParameters (constraint :312-320; class arm in reparseHosted JSDocTemplateTag case) | crates/tsr-checker/src/declared.rs:7922 local_type_parameters_of (+ tsr-parser/src/jsdoc.rs:534 parse_template_tag constraint=None on TypeParameterDeclaration) | 2 | 0 | 6 |
| 129 | `MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS` | instantiateMappedType's per-constituent arms are not ported: a homomorphic mapped type whose type variable maps to a still-generic T must stay a deferred mapped type, and a non-object (e.g. `object`) passes through uncha | internal/checker/checker.go:22535 instantiateMappedType (instantiateConstituent) | crates/tsr-checker/src/mapped.rs:783 instantiate_mapped_type_worker (L800 primitive-only passthrough; L838-866 unconditional member resolution/render) | 2 | 0 | 6 |
| 130 | `COMPARE-TYPES-ARMS` | TSR compare_types lacks two CompareTypes arms: the intersection arm (compareTypeLists of constituents) and compareTypeNames' alias-symbol name (getTypeNameSymbol returns t.alias.symbol first, so an aliased function type  | internal/checker/utilities.go:415 CompareTypes (intersection arm ~:504; compareTypeNames :589, getTypeNameSymbol :607) | crates/tsr-checker/src/unions.rs:1416 compare_types and :1325 compare_type_names (named_symbol_name :390) | 2 | 0 | 2 |
| 131 | `ARRAY-LITERAL-SUBTYPE-REDUCTION` | checkArrayLiteral always unions element types with UnionReductionSubtype; TSR check_array_literal_value only subtype-reduces when the union has more than one object constituent, so `[t, base]` (T extends Base) stays (T \| | internal/checker/checker.go:8021 checkArrayLiteral (getUnionTypeEx(elementTypes, UnionReductionSubtype) at :8096) | crates/tsr-checker/src/array_literals.rs:750 check_array_literal_value (object_constituent_count gate at :1136) | 1 | 0 | 18 |
| 132 | `MAPPED-TUPLE-TEMPLATE-INSTANTIATION` | instantiateMappedTupleType applies an arbitrary template per tuple element; TSR's alias sequence path only normalizes identity-like templates, leaving `Awaitified<[...]>` unresolved. | internal/checker/checker.go:22593 instantiateMappedTupleType | crates/tsr-checker/src/mapped.rs:896 instantiate_mapped_sequence (gated by declared.rs:3769 is_normalized_mapped_sequence) | 1 | 0 | 8 |
| 133 | `JSDOC-OVERLOAD-SIGNATURES` | Reparser turns each @overload JSDoc block preceding a method into an overload signature (implementation keeps its own JSDoc @param types); TSR models no @overload signatures and jsdoc_parameter_annotation declines entire | internal/parser/reparser.go:134 reparseUnhosted (KindJSDocOverloadTag arm; :233) | crates/tsr-checker/src/signatures.rs:1358 JS signature-from-JSDoc collection (no @overload arm) + crates/tsr-checker/src/symbols.rs:5660 jsdoc_parameter_annotat | 1 | 0 | 7 |
| 134 | `CLASS-TYPE-BASE-TYPE-VARIABLE-INTERSECTION` | getTypeOfFuncClassEnumModuleWorker intersects a class's static type with getBaseTypeVariableOfClass (base constructor type that is/contains a type variable), giving `{ new(...): (Anonymous class); prototype: ... } & TBas | internal/checker/checker.go:16912 getTypeOfFuncClassEnumModuleWorker (:16923-16927 class arm) / :16936 getBaseTypeVariableOfClass | crates/tsr-checker/src/symbols.rs:2732 get_type_of_func_class_enum_module_worker (class arm :2783-2791, decline in anonymous_class_written_name :3071-3100) | 1 | 0 | 6 |
| 135 | `GETTER-RETURN-ANNOTATION-CONTEXT` | getReturnTypeFromAnnotation's get-accessor arms (own return annotation, else paired setter's annotated param type) provide the contextual type of getter return expressions; TSR's ReturnStatement arm reads annotations onl | internal/checker/checker.go:20058 getReturnTypeFromAnnotation (getter arm :20066) | crates/tsr-checker/src/contextual.rs:1417 get_contextual_type ReturnStatement arm (annotation match omits GetAccessor) | 1 | 0 | 6 |
| 136 | `KEYOF-ANY-TYPE-NODE` | `keyof any` written as a type node yields error in TSR; getIndexType(any) is string \| number \| symbol. | internal/checker/checker.go:26684 getIndexTypeEx (any/never arm :26701) | crates/tsr-checker/src/declared.rs:621 get_type_from_type_node TypeOperatorNode keyof arm | 1 | 0 | 4 |
| 137 | `RENAMED-TYPE-PARAM-KEEPS-WRITTEN-NODE` | Upstream renames a shadowed signature type parameter (U -> U_1) as display-only remapping inside node building; reuse of the written return node survives the rename. TSR implements the rename by instantiate_signature wit | internal/checker/nodebuilderimpl.go:2023 serializeReturnTypeForSignature (reuse via pseudoTypeToNodeWithCheckerFallback -> tryReuseExistingNodeHelper with type- | crates/tsr-checker/src/inference.rs:5583 rename_type_parameters_for_site (calls instantiate_signature :5676, which clears written_return at :5874) | 1 | 0 | 4 |
| 138 | `ORIGIN-ENTRY-ORDER` | getUnionTypeWorker builds the denormalized origin by insertType (CompareTypes on the entries themselves: a named union sorts by its Union flag bit, after type params/conditionals); TSR sorts origin entries by their first | internal/checker/checker.go:25653 getUnionTypeWorker (origin arm :25705-25728, insertType :26621) | crates/tsr-checker/src/unions.rs:570 union_type_worker (entry key at :618-636) | 1 | 0 | 3 |
| 139 | `JS-LITERAL-PROPERTY-ANY` | Property access on a JS object-literal type (ObjectFlagsJSLiteral) with no such property and no index info returns any (unless unchecked-JS suggestion); TSR applies js_literal_types only to element access, so property ac | internal/checker/checker.go:11334 checkPropertyAccessExpressionOrQualifiedName (isJSLiteralType arm; utilities.go:1753 isJSLiteralType) | crates/tsr-checker/src/members.rs:73 check_property_access_expression | 1 | 0 | 2 |
| 140 | `CONDITIONAL-BRANCH-NAMED-UNION` | When the selected (infer) branch is a union containing a NAMED union (multi-member enum or union alias), TSR's branch instantiation declines and keeps the alias, tsgo instantiates the branch normally. | internal/checker/checker.go:24427 getConditionalType instantiateType(trueType, trueMapper) | crates/tsr-checker/src/declared.rs:7252 evaluate_conditional_inference | 1 | 0 | 2 |
| 141 | `IMPORT-CALL-SYNTHETIC-DEFAULT-TYPE` | checkImportCallExpression wraps the resolved module type with getTypeWithSyntheticDefaultOnly/getTypeWithSyntheticDefaultImportType; TSR's check_import_call_expression mints a bare `typeof import("m")` with no synthetic  | internal/checker/checker.go:8267 checkImportCallExpression (:8305-8311) | crates/tsr-checker/src/calls.rs:422 check_import_call_expression | 1 | 0 | 2 |
| 142 | `INFERENCE-SUPERTYPE-NAMED-UNION` | getCommonSupertype's literal path calls the real getUnionType, whose named-union arm returns the existing alias union (Bit) when the candidates are exactly its members; TSR covariant_combination uses get_union_type_unpri | internal/checker/inference.go:1530 getCommonSupertype -> checker.go:25653 getUnionTypeWorker (namedUnions[0] return) | crates/tsr-checker/src/inference.rs:7160 covariant_combination (get_union_type_unprinted at :7189/:7198) | 1 | 0 | 2 |
| 143 | `MAPPED-AS-CLAUSE-INDEX-TYPE` | getIndexTypeForMappedType for an as-clause mapped type maps each key through the instantiated name type; TSR's keyof leaves the name-type conditional uninstantiated per key. | internal/checker/checker.go:26871 getIndexTypeForMappedType | crates/tsr-checker/src/mapped.rs:461 mapped_index_type | 1 | 0 | 1 |
| 144 | `MAPPED-TEMPLATE-ADD-OPTIONALITY-INDEX` | Index infos produced by resolveMappedTypeMembers use getTemplateTypeFromMappedType (addOptionality for `?`), TSR builds them from the raw template. | internal/checker/checker.go:22697 getTemplateTypeFromMappedType | crates/tsr-checker/src/mapped.rs:595 resolve_mapped_type_members_worker | 1 | 0 | 1 |
| 145 | `TRYSYMBOLTABLE-UMD-ALIAS-EXCLUSION` | trySymbolTable excludes UMD `export as namespace` aliases when the enclosing file is a module; TSR's module_alias_at lacks that exclusion (best_name has it), so the module prints `typeof Foo` instead of falling back to i | internal/checker/symbolaccessibility.go:566 trySymbolTable (isUMDExportSymbol arm) | crates/tsr-checker/src/checker.rs:3380 module_alias_at | 1 | 0 | 1 |

## Ranked table: long tail (wave-1 only)

These are single-witness diagnoses from wave 1. Each one was minimised, but none was re-verified by a second agent. Treat each row as a lead with a named function.

| # | cluster | root cause (wave-1 definition) | tsgo | TSR | blocked | finished alone | lines |
|---|---|---|---|---|---|---|---|
| 1 | `RECURSION-DEPTH` | Instantiation of a recursive anonymous return type (`function foo<T>() { var z = foo<typeof y>(); var y: { y2: typeof z }; return y }`) proceeds until instantiationDepth / circularity yields any at de | internal/checker/checker.go:22100 instantiateType (instantiationDepth==100 / getTypeOfSymbol circularity -> any) | crates/tsr-checker/src (instantiate_type depth / circular var type -> `{}`) | 3 | 3 | 73 |
| 2 | `CONTEXTUAL-GENERIC-CONDITIONAL-CONSTRAINT` | When the contextual type of an argument is a generic (deferred) conditional type over an inferring type parameter, getApparentTypeOfContextualType maps it through getApparentType -> getBaseConstraintO | internal/checker/checker.go:30686 getApparentTypeOfContextualType (getApparentType -> computeBaseConstraint conditional  | crates/tsr-checker/src/contextual.rs apparent_contextual_type (~L591) -> base_constraint_of_type / constraints.rs defaul | 3 | 3 | 40 |
| 3 | `TAGGED-TEMPLATE-EFFECTIVE-ARGS` | Tagged templates resolve through the ordinary resolveCall with getEffectiveCallArguments = [synthetic TemplateStringsArray expression, ...span expressions] and the tag's this-argument. TSR's check_tag | internal/checker/checker.go:30042 getEffectiveCallArguments (tagged-template arm) -> checker.go:8843 resolveCall | crates/tsr-checker/src/calls.rs check_tagged_template_expression (~L1162, shifted overload pick at ~L1250) | 3 | 3 | 6 |
| 4 | `IMPORT-ATTRIBUTES-ALIAS-GATE` | Default-import alias target (getTargetOfImportClause -> getTargetOfModuleDefault) is resolved regardless of an import attributes clause. TSR import_clause_default_target refuses any ImportDeclaration  | internal/checker/checker.go:14528 getTargetOfImportClause / checker.go:14536 getTargetOfModuleDefault | crates/tsr-checker/src/symbols.rs import_clause_default_target (~L1372 `import.attributes.is_none()`; also ~L1619, ~L167 | 3 | 2 | 3 |
| 5 | `CONTEXTUAL-BINDING-PATTERN-INITIALIZER` | initializer of an unannotated variable with a binding-pattern name is contextually typed by getTypeFromBindingPattern(includePatternInType); feeds hasContextualTypeWithNoGenericTypes constraint substi | internal/checker/checker.go:29423 getContextualTypeForInitializerExpression | crates/tsr-checker/src/contextual.rs get_contextual_type VariableDeclaration arm (annotation only) | 2 | 2 | 26 |
| 6 | `OBJLIT-ACCESSOR-DEFERRED` | checkObjectLiteral only checkNodeDeferred's get/set accessors; their types resolve lazily, so a getter returning the variable itself is not circular | internal/checker/checker.go:13144 checkObjectLiteral (accessor arm :13314) | crates/tsr-checker/src/objects.rs object literal accessor member typing (eager) | 2 | 2 | 24 |
| 7 | `VARIANCE-CIRCULARITY` | Property initializer `callme(this).num` with overloaded callme over the class's own generic type resolves in tsgo (no circularity); TSR reports self-referential circularity. Not minimised. | internal/checker/checker.go (getVariances / resolveCall on this-typed arg) | crates/tsr-checker/src/symbols.rs property initializer circularity (resolutions stack) | 2 | 2 | 14 |
| 8 | `PLUS-ASSIGNABLE-TO-KIND` | `+` operator classification: checkBinaryLikeExpression asks isTypeAssignableToKindEx(operand, NumberLike, true), i.e. full assignability to `number` (for T[K] via getConstraintFromIndexedAccess -> Rec | internal/checker/checker.go:12336 checkBinaryLikeExpression (+ arm) -> :27645 isTypeAssignableToKindEx -> isTypeAssignab | crates/tsr-checker/src/binary.rs check_addition (L221-387, kind_source/has_kind flag tests) | 2 | 2 | 12 |
| 9 | `CONTEXTUAL-ARG-SUPER-CALL` | Arguments of a `super(...)` call are contextually typed via getContextualTypeForArgumentAtIndex -> getResolvedSignature(superCall) (resolveCallExpression's super arm: base constructor type with the ex | internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex -> checker.go:8471 resolveCallExpression (super ar | crates/tsr-checker/src/contextual.rs argument contextual type (no SuperCall callee path); calls.rs super call resolution | 2 | 2 | 12 |
| 10 | `NODEBUILDER-DIVERGENT-ACCESSOR` | Property whose getter/setter types differ (or class accessor w/o property decl) must print as `get p(): T; set p(v: W);` pair in object/type-literal printing. | internal/checker/nodebuilderimpl.go:2524 addPropertyToElementList (accessor arm: getWriteTypeOfSymbol != getNonMissingTy | crates/tsr-checker/src/printing.rs type_literal_text_at / object_literal_text_at (prints single `p: T` member) | 2 | 2 | 6 |
| 11 | `PARSER-ASTERISK-METHOD` | Parser recovery: in an object literal element, a `*` token forces parseMethodDeclaration even when the name is missing and no `(`/`<` follows (`{ *{ } }`, `{ * }`). TSR only takes the method arm on `( | internal/parser/parser.go:5625 parseObjectLiteralElement (:5650 `asteriskToken != nil \|\| token == ( \|\| token == <` -> pa | crates/tsr-parser/src/expression.rs parse_object_literal_element_worker (L1540 method arm ignores `asterisk`) | 2 | 2 | 6 |
| 12 | `AWAITED-THIS-TYPE` | Async function return wrapping when the body returns the polymorphic `this` type: getReturnTypeFromBody -> createPromiseReturnType(getAwaitedTypeNoAlias(this)); isAwaitedTypeNeeded reads getBaseConstr | internal/checker/checker.go:20372 createPromiseReturnType / :31266 getAwaitedTypeNoAlias / :31392 isAwaitedTypeNeeded (g | crates/tsr-checker/src/expressions.rs is_awaited_type_needed (L3192) / awaited_type (L3130) -> base_constraint_of_type f | 2 | 2 | 4 |
| 13 | `LOGICAL-ASSIGNMENT-OPERATOR` | &&=, \|\|=, ??= expression types (logical arms of checkBinaryLikeExpression shared with &&/\|\|/??). | internal/checker/checker.go:12336 checkBinaryLikeExpression (arms at 12496, 12518) | crates/tsr-checker/src/binary.rs:38 check_binary_expression (`_ => error` at 162; compound logical forms unhandled) | 2 | 2 | 3 |
| 14 | `DEFAULT-EXPORT-ALIAS-CLASS-MERGE` | `export default foo` (Alias) and `export default class Foo {}` merge into ONE `default` symbol (ClassExcludes does not exclude Alias); class type is created on it and printed with the first declaratio | internal/binder/binder.go:152 declareSymbolEx (no conflict -> merge) + internal/checker/nodebuilderimpl.go:973 getNameOf | crates/tsr-binder/src/binder.rs default-export declare / crates/tsr-checker/src/declared.rs declared type of ALIAS\|CLASS | 2 | 2 | 2 |
| 15 | `RELATER-APPARENT-INDEX-SIGNATURE` | Subtype relation from a primitive to an object type with a number index signature goes through the primitive's apparent type (String has [index: number]: string) and indexSignaturesRelatedTo; used by  | internal/checker/relater.go:4578 indexSignaturesRelatedTo; flow.go:794 isConstructedBy | crates/tsr-checker/src/relater.rs (index-signature subtype for primitive sources) via flow.rs is_constructed_by | 2 | 2 | 2 |
| 16 | `FLOW-INITIAL-BINDING-ELEMENT` | Flow assignment node for a destructuring binding element (var/let/for-of `{y: b = true}`): the initial type is getTypeWithDefault(parent element type, default) and getAssignmentReducedType narrows the | internal/checker/flow.go:2273 getInitialTypeOfBindingElement (via flow.go:276 getInitialOrAssignedType, flow.go:2389 get | crates/tsr-checker/src/flow.rs get_initial_or_assigned_type (~L2440) | 2 | 2 | 2 |
| 17 | `CONTEXTUAL-MAPPED-TUPLE-CONSTRAINT` | An array literal contextually typed by a homomorphic mapped type over a type variable with a tuple-like constraint (`T extends {0: unknown}`, `TTypes extends readonly [T, ...T[]]`) is checked in tuple | internal/checker/checker.go:8029 checkArrayLiteral inTupleContext; :21772 getResolvedApparentTypeOfMappedType; :29291 su | crates/tsr-checker/src/array_literals.rs check_array_literal (L647); contextual.rs contextual element/property types | 2 | 1 | 33 |
| 18 | `CONTEXTUAL-ARG-GENERIC-MAPPED` | Inside a generic call, an object-literal argument whose parameter type is a generic mapped type over an UNFIXED type parameter (`{[P in keyof U]: (props: X) => U[P]}`) must give each property the cont | internal/checker/checker.go:30551 getTypeOfPropertyOfContextualType (+ :29291 substituteIndexedMappedType, instantiateCo | crates/tsr-checker/src/contextual.rs contextual_type_for_argument (inferential/active_inference_contexts road) -> mapped | 2 | 1 | 19 |
| 19 | `LITERAL-WIDENING` | Object-literal property literal types are widened (getWidenedLiteralType via checkPropertyAssignment/checkExpressionForMutableLocation) when the contextual type does not contain the literal; `{ x: isR | internal/checker/checker.go:13878 checkExpressionForMutableLocation -> :25487 getWidenedLiteralType / isLiteralOfContext | crates/tsr-checker/src/widening.rs / expressions.rs mutable-location widening | 2 | 1 | 6 |
| 20 | `DECL-NAME-MERGED-SYMBOL` | The type of a declaration name is the type of the MERGED symbol (getMergedSymbol), so a local `interface Document` method shows lib + local signatures. | internal/checker/checker.go:14390 getSymbolOfDeclaration -> 14355 getMergedSymbol (getTypeOfNode 31927) | crates/tsr-conformance/src/types_producer.rs declaration-name road (binder symbol, not merged); symbol_access.rs:557 mer | 2 | 1 | 6 |
| 21 | `KEYOF-ANY` | getIndexType(any) = keyofConstraintType (string \| number \| symbol); TSR's `keyof any` is error. | internal/checker/checker.go:26680 getIndexType | crates/tsr-checker/src/declared.rs TypeOperator keyof arm | 2 | 1 | 5 |
| 22 | `IDENT-UNDEFINED-SHADOWED` | The identifier `undefined` resolves by ordinary name resolution: a local class/namespace named undefined shadows the global, and a qualifier `undefined` inside a type reference qualified name is not a | internal/checker/checker.go:11042 checkIdentifier (undefinedSymbol identity check), :31927 getTypeOfNode | crates/tsr-checker/src/expressions.rs identifier resolution / crates/tsr-conformance/src/types_producer.rs type_at_locat | 2 | 1 | 3 |
| 23 | `NODEBUILDER-SHADOWED-TYPEPARAM-CONST` | Overloaded function whose overloads redeclare the same type-parameter name with a `const` modifier: the function type is the ordinary overload set; the node builder renames shadowed params (T_1, Gener | internal/checker/nodebuilderimpl.go:1404 typeParameterToName (shadowed-name generation) via signatureToSignatureDeclarat | crates/tsr-checker/src/checker.rs type_parameter_name_at / signatures.rs type_parameter_of (const admitted) — combinatio | 1 | 1 | 48 |
| 24 | `APPARENT-UNKNOWN-NONSTRICT` | getApparentType of an unconstrained type parameter: constraint unknown -> emptyObjectType {} when strictNullChecks is off, so Object members (toString) resolve. | internal/checker/checker.go:21729 getApparentType (21758 unknown && !strictNullChecks) | crates/tsr-checker/src/members.rs:1113 apparent_type | 1 | 1 | 30 |
| 25 | `SELF-REFERENTIAL-TYPE-LITERAL` | A type literal whose member references the declaring variable (`var a: { foo: typeof a }`): tsgo creates ONE anonymous type with lazily resolved members (resolveAnonymousTypeMembers), so `typeof a` in | internal/checker/checker.go:22933 getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode + :20650 resolveAnonymousTypeMem | crates/tsr-checker/src/declared.rs get_type_from_type_literal (eager member typing, text baked at creation) + TypeQuery  | 1 | 1 | 19 |
| 26 | `CONTEXTUAL-PARAM` | Unannotated callback parameter in object literal not contextually typed; missing source: discriminated contextual type (discriminateContextualTypeByObjectMembers on `disc: undefined` / absent disc). | internal/checker/checker.go discriminateContextualTypeByObjectMembers / getApparentTypeOfContextualType | crates/tsr-checker/src/contextual.rs | 1 | 1 | 16 |
| 27 | `WRITE-TYPE-INSTANTIATION` | getWriteTypeOfSymbol on an instantiated property symbol must instantiate the target's setter type with the symbol mapper (getWriteTypeOfInstantiatedSymbol). | internal/checker/checker.go:16536 getWriteTypeOfInstantiatedSymbol (from getWriteTypeOfSymbol checker.go:16434) | crates/tsr-checker/src/symbols.rs write_type_of_accessors (returns uninstantiated setter type) | 1 | 1 | 15 |
| 28 | `INFERENCE-INDEXED-ACCESS` | inferFromTypes arm: source and target both IndexedAccess (A[B] -> T[K]) infer objectType->objectType and indexType->indexType. TSR lacks it, so find<T,K extends keyof T>(o: T[K]) called with A[B] fail | internal/checker/inference.go:240 inferFromTypes (IndexedAccess/IndexedAccess arm; func at :65) | crates/tsr-checker/src/inference.rs infer_from_types (no indexed-access pair arm) | 1 | 1 | 14 |
| 29 | `LAZY-FUNCEXPR-RETURN-CIRCULARITY` | A function expression's type is an anonymous type whose signature return type is resolved lazily (getReturnTypeOfSignature with its own resolution stack); a variable initialized with an arrow that cal | internal/checker/checker.go:10114 checkFunctionExpressionOrObjectLiteralMethod; :20001 getReturnTypeOfSignature; :20126  | crates/tsr-checker/src/signatures.rs get_type_of_function_expression (L6149) / get_return_type_from_body (L2407) | 1 | 1 | 12 |
| 30 | `NARROW-EQUALITY-REPLACE-PRIMITIVES` | narrowTypeByEquality assume-true arm must call replacePrimitivesWithLiterals even when the comparable filter keeps every constituent (string === "foo" -> "foo"). | internal/checker/flow.go:556 narrowTypeByEquality -> flow.go:1907 replacePrimitivesWithLiterals | crates/tsr-checker/src/flow.rs:7356-7365 (returns t when kept.len()==total, skipping replace_primitives_with_literals) | 1 | 1 | 10 |
| 31 | `NOLIB-GLOBAL-TYPE-FALLBACK` | missing global types fall back to emptyGenericType/emptyObjectType (array types print `{}`) | internal/checker/checker.go:1210 getGlobalType | crates/tsr-checker/src/intrinsics/global type lookup | 1 | 1 | 9 |
| 32 | `RETURN-LITERAL-SELF-CONTEXTUAL-SIG` | getReturnTypeFromBody: if the function's contextual signature IS its own signature (method in an object literal passed to a bare type parameter, which is inferred from the literal itself), the unit re | internal/checker/checker.go:20203-20222 getReturnTypeFromBody (case contextualSignature == getSignatureFromDeclaration(f | crates/tsr-checker/src/signatures.rs contextual_return_widening_type (contextual.rs L1560) as used at signatures.rs L367 | 1 | 1 | 9 |
| 33 | `ARRAY-LITERAL-TUPLE-LIKE-CONTEXT` | Array literal is in tuple context when the contextual type isTupleLikeType, which includes any type with a property "0" (RegExpMatchArray), not only tuples. | internal/checker/checker.go:23544 isTupleLikeType (used by checkArrayLiteral inTupleContext) | crates/tsr-checker/src/array_literals.rs:608 array_literal_has_a_tuple_contextual_type (tuple_element_lists only) | 1 | 1 | 7 |
| 34 | `CONTEXTUAL-NESTED-OPTIONAL-PROPERTY` | getContextualTypeForObjectLiteralElement -> getTypeOfPropertyOfContextualType(mapType over `T \| undefined`) for an object literal nested under an OPTIONAL property whose own member is also optional (` | internal/checker/checker.go:29920 getContextualTypeForObjectLiteralElement -> :30555 getTypeOfPropertyOfContextualTypeEx | crates/tsr-checker/src/contextual.rs contextual_type_for_object_literal_named_element / union_contextual_property_type ( | 1 | 1 | 7 |
| 35 | `CONTEXTUAL-LITERAL-TYPE-PARAM-CONSTRAINT` | Object-literal property initializer literal is kept (not widened) when its contextual type, obtained through a type-parameter contextual type's constraint (getApparentTypeOfContextualType -> property  | internal/checker/checker.go:25515 getWidenedLiteralLikeTypeForContextualType / 25522 isLiteralOfContextualType (via chec | crates/tsr-checker/src/contextual.rs contextual type of object-literal property under a type-parameter contextual type / | 1 | 1 | 7 |
| 36 | `DEFERRED-TYPE-LITERAL-MEMBERS` | Type-literal member types are resolved lazily (resolveAnonymousTypeMembers), so `declare var a: {prop:number}\|{prop:T27}; type T27 = typeof a` is not circular. | internal/checker/checker.go:22933 getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode, :20650 resolveAnonymousTypeMemb | crates/tsr-checker/src/declared.rs get_type_from_type_literal (eager member types -> circularity) | 1 | 1 | 6 |
| 37 | `FLOW-ASSIGNED-FOROF` | Flow assignment type for an existing variable used as a for-of/for-in initializer (`for (x of arr)`): upstream's getAssignedType ForOf arm answers the iterated element type (checkRightHandSideOfForOf) | internal/checker/flow.go:2288 getAssignedType (KindForOfStatement arm :2293, KindForInStatement) | crates/tsr-checker/src/flow.rs get_initial_or_assigned_type (only VariableDeclaration initializer and plain `x = e`) | 1 | 1 | 6 |
| 38 | `ASSIGN-READONLY-ERRORTYPE` | Assignment target that is an import alias (checkIdentifier: Cannot_assign_to_0_because_it_is_an_import) or a readonly entity such as a namespace-import member (checkPropertyAccessExpressionOrQualified | internal/checker/checker.go:11042 checkIdentifier (~L11088-11094) and checker.go:11258 checkPropertyAccessExpressionOrQu | crates/tsr-checker/src/expressions.rs identifier / property-access check in assignment-target position | 1 | 1 | 6 |
| 39 | `ACCESSORS-MERGED-PROPERTY` | Symbol merging a PropertyDeclaration with get/set accessors (binder allows it): getTypeOfSymbol takes the Accessor arm first; getTypeOfAccessors ignores non-accessor declarations and infers from the g | internal/checker/checker.go:16506 getTypeOfSymbol (Accessor arm before Property) -> checker.go:18511 getTypeOfAccessors | crates/tsr-checker/src/symbols.rs get_type_of_accessors_worker (`other => error` at ~L342) | 1 | 1 | 6 |
| 40 | `CONTEXTUAL-MODULE-EXPORTS-ASSIGN` | RHS of `module.exports = expr` has no contextual type; TSR derives one from the left side (circular), erroring function expressions. | internal/checker/checker.go:29820 getContextualTypeForBinaryOperand / 29850 getContextualTypeForAssignmentExpression | crates/tsr-checker/src/contextual.rs binary-operand contextual type | 1 | 1 | 5 |
| 41 | `INFERENCE-APPARENT-SOURCE` | inferFromTypes default arm: a primitive source is replaced by its apparent type (String) before inferring to object targets (Iterable<T>). | internal/checker/inference.go:267 inferFromTypes (getApparentType) | crates/tsr-checker/src/inference.rs | 1 | 1 | 5 |
| 42 | `INTERSECTION-UNION-DISTRIBUTE-REDUCE` | getIntersectionType distributes over a union constituent and getReducedType removes members with never discriminant props | internal/checker/checker.go:26056 getIntersectionTypeEx / :21819 getReducedType | crates/tsr-checker/src/intersection construction | 1 | 1 | 5 |
| 43 | `CONTEXTUAL-DISCRIMINATE-OBJLIT` | discriminateContextualTypeByObjectMembers narrows an object literal's union contextual type using possibly-discriminant initializers (incl. template expressions) via discriminateTypeByDiscriminableIte | internal/checker/checker.go:30755 discriminateContextualTypeByObjectMembers, :30802 isPossiblyDiscriminantValue | crates/tsr-checker/src/symbols.rs discriminate_union_root (string/numeric/boolean literal initializers only) | 1 | 1 | 5 |
| 44 | `TYPELIT-PRIVATE-NAME-MEMBER` | Type literals/interfaces containing #private members (a grammar error) still resolve: members get mangled symbols and nested ones print; TSR answers error for the whole type literal. | internal/binder (private-name symbol names) + checker resolveAnonymousTypeMembers; nodebuilder member printing | crates/tsr-checker/src/declared.rs type-literal arm (PrivateIdentifier member -> error) | 1 | 1 | 5 |
| 45 | `RESOLVE-DECORATOR-SCOPE` | Names in decorators resolve outside the decorated declaration: class decorators don't see class type params; method/parameter decorators don't see the method's parameters. | internal/binder/nameresolver.go:245 (KindDecorator arm in Resolve) | crates/tsr-binder resolve_name (no decorator arm) | 1 | 1 | 4 |
| 46 | `VAR-TYPE-FROM-VALUE-DECLARATION` | Type of a merged (redeclared) var symbol comes from symbol.ValueDeclaration (the FIRST declaration) for every declaration/reference; TSR types each declaration from its own initializer. | internal/checker/checker.go:16578 getTypeOfVariableOrParameterOrPropertyWorker (uses symbol.ValueDeclaration) | crates/tsr-checker/src/symbols.rs type of variable symbol per declaration (declaration-local initializer) | 1 | 1 | 4 |
| 47 | `NARROW-INSTANCEOF-CONSTRUCT-SIGNATURES` | instanceof narrowing with a right operand that is not a class constructor: getInstanceType uses the `prototype` property type, else the union of construct-signature return types (type literal `{ new() | internal/checker/flow.go:811 narrowTypeByInstanceof -> :966 getInstanceType | crates/tsr-checker/src/flow.rs instanceof arm (~L5127-5274: TypeData::Anonymous class-identity branch / signature_candid | 1 | 1 | 4 |
| 48 | `GENERIC-INTERSECTION-ITERATION-ARRAYLIKE` |  |  |  | 1 | 1 | 4 |
| 49 | `INFERENCE-KEYOF-LITERAL` | inferFromTypes with an index-type target (`keyof T`) and a string-literal source: infers createEmptyObjectTypeFromStringLiteral(source) (`{ a: any }`) to T with InferencePriorityLiteralKeyof, contrava | internal/checker/inference.go:238 inferFromTypes keyof arm (+ :1229 createEmptyObjectTypeFromStringLiteral) | crates/tsr-checker/src/inference.rs infer_from_types (no keyof-target arm) | 1 | 1 | 4 |
| 50 | `TYPEFACTS-NONSTRICT-PRIMITIVES` | getTypeFactsWorker picks the non-strict fact tables (Base*Facts incl. Falsy/EQUndefined/EQNull) for primitives & literals when strictNullChecks is off, so falsy-narrowing keeps truthy literals. | internal/checker/checker.go:30982 getTypeFactsWorker (strictNullChecks ? *StrictFacts : *Facts) | crates/tsr-checker/src/flow.rs get_type_facts (literal/primitive arms use strict facts unconditionally, ~L8105) | 1 | 1 | 4 |
| 51 | `ARGUMENTS-NAME-RESOLVE` | `arguments` resolves to argumentsSymbol only inside non-arrow function-like declarations (nameresolver); elsewhere (interface members, function types, top level) it is unresolved -> errorType (prints  | internal/binder/nameresolver.go:228 (arguments arm) | crates/tsr-checker/src/expressions.rs:932-981 (unresolved `arguments` answers gap error, poisoning the signature) | 1 | 1 | 3 |
| 52 | `INSTANTIATE-SINGLE-GENERIC-CALL-SIGNATURE` | A generic-function-typed expression (identity) in a position whose contextual type is a single non-generic signature is instantiated in the context of that signature (instantiateSignatureInContextOf), | internal/checker/checker.go:7599 instantiateTypeWithSingleGenericCallSignature | crates/tsr-checker/src/inference.rs instantiateTypeWithSingleGenericCallSignature port (~L2241/2296) | 1 | 1 | 3 |
| 53 | `APPARENT-INDEXED-ACCESS-MAPPED-INTERSECTION` | Property access/destructuring on a generic indexed access whose object is an intersection of generic mapped types `(E<T> & S<T>)[keyof T]` resolves members through getApparentType/base constraint (map | internal/checker/checker.go:27915 getSimplifiedIndexedAccessType / computeBaseConstraint IndexedAccess arm (getApparentT | crates/tsr-checker/src/constraints.rs compute_base_constraint / mapped.rs indexed mapped substitution | 1 | 1 | 3 |
| 54 | `CONTEXTUAL-UNION-PROPERTY` | getTypeOfPropertyOfContextualType over a union contextual type maps constituents and skips those lacking the property, so `convert` gets (t: string) => string from the one constituent that declares it | internal/checker/checker.go:30551 getTypeOfPropertyOfContextualType | crates/tsr-checker/src/contextual.rs union_contextual_property_type / contextual_property_type | 1 | 1 | 3 |
| 55 | `COMPARABLE-TEMPLATE-LITERAL` | Discriminant narrowing where a constituent's discriminant property is a template-literal type (`${string} is wrong!`): upstream's narrowTypeByDiscriminant filter asks isTypeComparableTo(narrowedPropTy | internal/checker/flow.go:725 narrowTypeByDiscriminant (isTypeComparableTo filter) / relater.go templateLiteral relations | crates/tsr-checker/src/flow.rs narrow_type_by_discriminant -> discriminant_keeps (relate_ternary Unknown -> None -> retu | 1 | 1 | 3 |
| 56 | `GENERIC-ARG-FINAL-RECHECK` | Expression types inside a generic call's (context-sensitive/object-literal) argument must reflect the FINAL contextual type (instantiated signature); TSR records the types computed during the inferent | internal/checker/checker.go:8843 resolveCall final pass / 7484 checkExpressionWithContextualType; getContextualThisParam | crates/tsr-checker/src/calls.rs / inference.rs inferential argument checking (node-type cache keeps first-pass answers) | 1 | 1 | 3 |
| 57 | `NODEBUILDER-ANON-CLASS-NAME` | Type-reference printing of a nameless class-expression symbol uses getNameOfSymbolAsWritten's nameless-declaration arm -> `(Anonymous class)`; TSR applies it only to the `typeof` leg, so the instance  | internal/checker/nodebuilderimpl.go:1004-1014 getNameOfSymbolAsWritten | crates/tsr-checker/src/symbols.rs anonymous_class_written_name (not used by the generic type-reference printer in printi | 1 | 1 | 3 |
| 58 | `INFERENCE-CLASS-STATIC-CONSTRUCT-SIGS` | inferFromObjectTypes -> inferFromSignatures(kind Construct) when the TARGET is a class constructor type (static side of a generic-scoped class expression): its default construct signature (getDefaultC | internal/checker/inference.go:699 inferFromObjectTypes -> inference.go:838 inferFromSignatures (SignatureKindConstruct); | crates/tsr-checker/src/inference.rs object/signature inference (no construct-signature view of a class static type targe | 1 | 1 | 3 |
| 59 | `OBJLIT-PATTERN-OPTIONAL` | An object literal contextually typed by a binding pattern's implied type marks properties optional where the pattern has a default; must work through ParenthesizedExpression. | internal/checker/checker.go checkObjectLiteral (implied-type optional flag) + getContextualType parenthesized arm | crates/tsr-checker/src/binding_patterns.rs / contextual.rs (parenthesized initializer not given pattern context) | 1 | 1 | 2 |
| 60 | `REVERSE-MAPPED-TEMPLATE-INFERENCE` |  |  |  | 1 | 1 | 2 |
| 61 | `ENUM-RELATION-SAME-NAME` | isEnumTypeRelatedTo for two distinct enums with the same NAME requires every source member to exist in target with equal value; TSR leaves the same-name case undecided and subtype reduction collapses  | internal/checker/relater.go:282 isEnumTypeRelatedTo | crates/tsr-checker/src/relater.rs ~L1646-1659 (same_name -> None, 'not ported') | 1 | 1 | 2 |
| 62 | `TUPLE-NORMALIZE-NAMED-VARIADIC` | A tuple type node whose variadic element is a LABELLED member over a generic type (`[...args: T]`, `[...args: {..}[SS]]`) must normalise to a generic variadic tuple `[...args: T]` (createNormalizedTup | internal/checker/checker.go:24115 getTypeFromArrayOrTupleTypeNode -> :23351 createNormalizedTupleType | crates/tsr-checker/src/declared.rs get_type_from_tuple_type_node / tuple_type_node_structural (RestTypeNode(NamedTupleMe | 1 | 1 | 2 |
| 63 | `INFERENCE-KEYOF-STRING-LITERAL` | inferFromTypes arm: a string-literal (union) source inferred to a `keyof T` target creates an empty object type with those keys typed any (createEmptyObjectTypeFromStringLiteral) as a LiteralKeyof-pri | internal/checker/inference.go:238 inferFromTypes (keyof target arm) -> :1229 createEmptyObjectTypeFromStringLiteral | crates/tsr-checker/src/inference.rs infer_from_types (InferencePriority::LITERAL_KEYOF declared, arm missing) | 1 | 1 | 2 |
| 64 | `GLOBAL-SCRIPT-INTERFACE-MERGE` | A script-level (non-module) `interface Generator<T>` merges with the lib's global Generator<T, TReturn = any, TNext = any>; references then carry the merged symbol's 3 type parameters with defaults fi | internal/checker/checker.go mergeSymbolTable(globals) + :23169 getTypeFromClassOrInterfaceReference (fillMissingTypeArgu | crates/tsr-checker/src/declared.rs local_type_parameters_of / global merge of script declarations | 1 | 1 | 2 |
| 65 | `JSX-ELEMENT-TYPE-UNRESOLVED` | When JSX.Element cannot be resolved, getJsxElementTypeAt returns errorType — a real type that composes (array of it is any[]); TSR leaves a gap that poisons enclosing expressions. | internal/checker/jsx.go:1275 getJsxElementTypeAt / jsx.go:72 checkJsxElement | crates/tsr-checker/src/jsx checking | 1 | 1 | 2 |
| 66 | `NARROW-TYPEOF-DISCRIMINANT-OPTCHAIN` | typeof x?.prop narrowing of a union declared as AliasInstantiation \| null: optionalChainContainsReference + getDiscriminantPropertyAccess + narrowTypeByDiscriminant; TSR fails only for alias-instantia | internal/checker/flow.go:614 narrowTypeByTypeof | crates/tsr-checker/src/flow.rs typeof narrowing / discriminant access | 1 | 1 | 2 |
| 67 | `ACCESSOR-CIRCULAR-ANY` | getTypeOfAccessors: a getter whose return body reads itself hits a resolution cycle and yields any (implicit-any error). TSR answers no type. | internal/checker/checker.go:18511 getTypeOfAccessors | crates/tsr-checker/src/symbols.rs get_type_of_accessors (L219) | 1 | 1 | 2 |
| 68 | `SATISFIES-ERROR-TARGET` | checkSatisfiesExpression returns the target type itself when it is an error type (an unresolved `bleh` prints as `bleh`), otherwise the operand type. TSR always returns the operand type. | internal/checker/checker.go:10741 checkSatisfiesExpression (:10746-10748) | crates/tsr-checker/src/expressions.rs Expression::SatisfiesExpression arm (L988, 'TRANSPARENT') | 1 | 1 | 2 |
| 69 | `INFERENCE-UNION-TUPLE-TARGET` | Inferring from a tuple source to a union of tuple targets of different arity ([T] \| [T, U]) via inferToMultipleTypes; uninferred U falls to unknown. | internal/checker/inference.go:448 inferToMultipleTypes, :699 inferFromObjectTypes | crates/tsr-checker/src/inference.rs (union target inference) | 1 | 1 | 2 |
| 70 | `INFERENCE-MATCHING-UNION-IDENTITY` | inferFromTypes union-to-union step (inferFromMatchingTypes with identical/closely-matched constituents) must strip the shared `undefined` from T \| undefined <- "admin" \| undefined; fails in TSR only w | internal/checker/inference.go:65 inferFromTypes (union target arm) | crates/tsr-checker/src/inference.rs union inference + signatures.rs get_return_type_from_body (union identity) | 1 | 1 | 2 |
| 71 | `SPREAD-OPTIONAL-MISSING` | getSpreadType copies optional properties with their declared type plus missingType (printed without `\| undefined` when exactOptionalPropertyTypes is off); TSR adds plain undefined. | internal/checker/checker.go:13387 getSpreadType, :13585 getSpreadSymbol | crates/tsr-checker/src/spreads.rs get_spread_type | 1 | 1 | 2 |
| 72 | `ARRAY-LITERAL-ERROR-ELEMENT` | checkArrayLiteral with an errorType element: tsgo unions the element types (errorType) and still creates Array<errorType>, printed `any[]`. TSR's check_array_literal_value returns intrinsics.error as  | internal/checker/checker.go:8021 checkArrayLiteral (createArrayLiteralType(createArrayType(getUnionType(elementTypes)))) | crates/tsr-checker/src/array_literals.rs check_array_literal_value (L795-809 `if self.is_error(t) { return error }`) | 1 | 1 | 2 |
| 73 | `AUTO-TYPE-UNDEFINED-INIT` | getTypeForVariableLikeDeclaration's autoType arm (noImplicitAny): a non-const variable whose initializer isNullOrUndefined — including the IDENTIFIER `undefined`, not just `null` — gets control-flow-t | internal/checker/checker.go:16697-16705 getTypeForVariableLikeDeclaration (autoType arm, c.isNullOrUndefined(initializer | crates/tsr-checker/src/flow.rs is_auto_typed_declaration (L2116) / symbols.rs autoType arm (~L5350) | 1 | 1 | 2 |
| 74 | `INDEX-SIG-INVALID-OR-UNTYPED` | Index signatures with an invalid key type are dropped (isValidIndexKeyType) and a missing value annotation means any; the type literal still resolves. | internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (19650-19658) | crates/tsr-checker/src/index_signatures.rs:375 index_infos_of_symbol / declared.rs type literal | 1 | 1 | 2 |
| 75 | `INFERENCE-MATCHING-INTERSECTION-CONSTITUENTS` | inferFromTypes with an intersection target and intersection source: constituents identical in source and target are matched and removed (inferFromMatchingTypes with isTypeIdenticalTo) before inferring | internal/checker/inference.go:73 inferFromTypes (intersection arm) -> inference.go:369 inferFromMatchingTypes | crates/tsr-checker/src/inference.rs infer_from_types (intersection target) | 1 | 1 | 2 |
| 76 | `QUICK-TYPE-OF-EXPRESSION` | Writer's getTypeOfNode->getRegularTypeOfExpression->getTypeOfExpression takes the getQuickTypeOfExpression fast path: a call/new with a single non-generic signature answers that signature's return typ | internal/checker/checker.go:7361 getQuickTypeOfExpression (via 7337 getTypeOfExpression, 32111 getRegularTypeOfExpressio | crates/tsr-conformance/src/types_producer.rs:1577 type_id_at_location_tracking calls checker.check_expression (no quick  | 1 | 1 | 1 |
| 77 | `UNION-TUPLE-INDEX-INFO` | Index infos of a union whose constituents are tuples (number index from Array<E>) via getUnionIndexInfos. | internal/checker/checker.go:13510 getUnionIndexInfos | crates/tsr-checker/src/index_signatures.rs:302 union_index_infos (tuple constituents yield None) | 1 | 1 | 1 |
| 78 | `ACCESSOR-WRITE-TYPE-ELEMENT-ACCESS` | Element access used as assignment target reads the property's WRITE type (set accessor parameter) for computed/unique-symbol keys. | internal/checker/checker.go getIndexedAccessType with AccessFlagsWriting -> getWriteTypeOfSymbol | crates/tsr-checker/src/indexed.rs (read type used) | 1 | 1 | 1 |
| 79 | `WITH-STATEMENT-BODY-ERROR` | getTypeOfNode returns errorType for any node with NodeFlagsInWithStatement (inside a `with` body), including declaration names. | internal/checker/checker.go:31932 getTypeOfNode (NodeFlagsInWithStatement -> errorType) | crates/tsr-conformance/src/types_producer.rs / crates/tsr-checker/src/check.rs in-with-statement predicate (~L13405) not | 1 | 1 | 1 |
| 80 | `NARROWABLE-REF-BINDING-PATTERN-CONTEXT` | getNarrowableTypeForReference substitutes a generic reference's union constraint when hasContextualTypeWithNoGenericTypes holds; for the initializer of a destructuring declaration the contextual type  | internal/checker/checker.go:31491 getNarrowableTypeForReference / :31533 hasContextualTypeWithNoGenericTypes (getContext | crates/tsr-checker/src/constraints.rs narrowable_type_for_reference (concrete_context via get_contextual_type) + context | 1 | 1 | 1 |
| 81 | `CONDITIONAL-INFER-TO-MAPPED` | Conditional-type inference (inferTypes for `infer U`) into a mapped-type extends target `{ [_ in keyof T]: infer U }`: inferToMappedType infers U from the union of the source's property (and index) ty | internal/checker/inference.go:948 inferToMappedType (via getConditionalType inferTypes) | crates/tsr-checker/src/inference.rs mapped-target inference (infer_to_mapped_type equivalent) / declared.rs evaluate_con | 1 | 1 | 1 |
| 82 | `INFER-TYPEPARAM-IMPLIED-CONSTRAINT` | An `infer B` placed as a type argument of a generic alias/reference (`SubGuard<M[number], infer B>` with `X extends [A]`) gets an implied constraint from that type parameter's constraint (getInferredT | internal/checker/checker.go:17114 getInferredTypeParameterConstraint (via :17071 getConstraintFromTypeParameter) | crates/tsr-checker/src/declared.rs conditional evaluation of infer type parameters (no implied-constraint port) | 1 | 1 | 1 |
| 83 | `NARROWABLE-REF-CONTEXTUAL` | getNarrowableTypeForReference substitutes union-constraint of a generic reference when hasContextualTypeWithNoGenericTypes; for a call argument after resolution the contextual type is the instantiated | internal/checker/checker.go:31491 getNarrowableTypeForReference / 31533 hasContextualTypeWithNoGenericTypes | crates/tsr-checker/src/constraints.rs:240 narrowable_type_for_reference (get_contextual_type sees generic/uninstantiated | 1 | 1 | 1 |
| 84 | `KEYOF-UNIQUE-SYMBOL-KEYS` | keyof includes unique-symbol (late-bound) property keys (getLiteralTypeFromProperties include ESSymbolLike). | internal/checker/checker.go:26717 getLiteralTypeFromProperties / 26680 getIndexType | crates/tsr-checker/src/keyof evaluation | 1 | 1 | 1 |
| 85 | `INFERENCE-MAPPED-ENUM-OBJECT-SOURCE` | inferToMappedType (constraint is a type param K): V inferred from the union of the source's property types; for an enum object source this is the enum type. | internal/checker/inference.go:948 inferToMappedType | crates/tsr-checker/src/inference.rs mapped-type inference (enum object properties) | 1 | 1 | 1 |
| 86 | `NARROWED-TYPE-UNKNOWN-FALSE-BRANCH` | false branch of a type predicate on unknown: getNarrowedTypeWorker expands to unknownUnionType and filters constituents that are subsets of the true type -> null \| undefined; TSR returns unknown | internal/checker/flow.go:859 getNarrowedTypeWorker | crates/tsr-checker/src/flow.rs narrowed_type_worker / narrowed_constituent | 1 | 1 | 1 |
| 87 | `TYPE-LITERAL-COMPUTED-ACCESSOR` | type literal containing a get/set accessor with a computed (non-literal) name still yields an object type; TSR declared type is error | internal/checker/checker.go:20894-region resolveAnonymousTypeMembers / getMembersOfSymbol (late-bound names) | crates/tsr-checker/src/declared.rs get_type_from_type_literal accessor members | 1 | 1 | 1 |
| 88 | `MERGE-SYMBOL-CONFLICT-NO-MERGE` | mergeSymbol with excluded flags (type alias into interface+namespace) reports an error and does NOT merge/record, so the alias keeps its own symbol | internal/checker/checker.go:14146 mergeSymbol | crates/tsr-binder (global merge) / checker merged_symbol | 1 | 1 | 1 |
| 89 | `ALIAS-CIRCULAR-ANY` | resolveAlias on an import=/export= cycle returns unknownSymbol after reporting circularity; getTypeOfAlias yields errorType which the writer prints as `any`. TSR has no type for the alias. | internal/checker/checker.go:16266 resolveAlias; :18598 getTypeOfAlias | crates/tsr-checker/src/symbols.rs get_type_of_alias (L409) / resolve_alias (L943) | 1 | 1 | 1 |
| 90 | `REVERSE-MAPPED-PROPERTY-ORDER` | Reverse-mapped type members follow getPropertiesOfType(source) declaration order. TSR emits them reversed when the source is a NAMED type (alias of type literal or interface); inline literal sources a | internal/checker/inference.go:1099 resolveReverseMappedTypeMembers; :1014 createReverseMappedType | crates/tsr-checker/src/inference.rs reverse_mapped_member_plan (L3456; property_names_of(source) order) | 1 | 1 | 1 |
| 91 | `REST-TUPLE-ALIAS-NORMALIZE` |  |  |  | 1 | 1 | 1 |
| 92 | `BINARY-PLUS-CONSTRAINT` | `+`/`+=` arm of checkBinaryLikeExpression classifies operands by isTypeAssignableToKind on base constraints; T[K] with T extends Record<K, number> is NumberLike -> number. | internal/checker/checker.go:12414 checkBinaryLikeExpression (PlusToken arm) | crates/tsr-checker/src/binary.rs check_binary_expression | 1 | 1 | 1 |
| 93 | `ASYNC-ACCESSOR-FUNCTION-FLAGS` | ast.GetFunctionFlags only reads `async`/`*` for function declarations/expressions, methods and arrows — an (invalid) `async` modifier on a get accessor is ignored, so getReturnTypeFromBody infers the  | internal/ast/functionflags.go:13 GetFunctionFlags; internal/checker/checker.go:20126 getReturnTypeFromBody via :18511 ge | crates/tsr-checker/src/symbols.rs get_type_of_accessors_worker (L287) -> get_return_type_from_body async arm | 1 | 1 | 1 |
| 94 | `PRIVATE-IDENTIFIER-EXPRESSION` | checkPrivateIdentifierExpression: a bare `#x` used as the left operand of `#x in obj` always types as anyType (after resolving the symbol for reference marking). TSR's check_expression has no PrivateI | internal/checker/checker.go:7837 checkPrivateIdentifierExpression | crates/tsr-checker/src/expressions.rs check_expression (no Expression::PrivateIdentifier arm) | 1 | 1 | 1 |
| 95 | `ENUM-EVAL-TEMPLATE` | Enum member initializer that is a template expression WITH substitutions (`1${"2"}3`): upstream's constant evaluator folds TemplateExpression (head + evaluated spans) to the string "123", and getEnumL | internal/evaluator/evaluator.go:108 evaluate (KindTemplateExpression arm) via internal/checker/checker.go:23938 computeE | crates/tsr-checker/src/declared.rs get_declared_type_of_enum (sequential constant folder, ~L5552 handles NoSubstitutionT | 1 | 1 | 1 |
| 96 | `IDENTIFIER-ASSIGN-NONVARIABLE` | checkIdentifier on an assignment target whose symbol is not a variable (undefined, enum, class, function...) reports and returns errorType. | internal/checker/checker.go:11077 checkIdentifier (assignment arm) | crates/tsr-checker/src/expressions.rs identifier checking (no non-variable assignment-target arm) | 1 | 1 | 1 |
| 97 | `MERGED-FUNCTION-INTERFACE` | A symbol merging a `declare function Tag` with an `interface Tag<...>` (SymbolFlags FUNCTION\|INTERFACE): getTypeOfSymbol takes the getTypeOfFuncClassEnumModule arm for the value and getDeclaredTypeOfS | internal/checker/checker.go:16904 getTypeOfFuncClassEnumModule / :23169 getTypeFromClassOrInterfaceReference | crates/tsr-checker/src/symbols.rs get_type_of_func_class_enum_module(_worker) + declared.rs generic reference to a merge | 3 | 0 | 15 |
| 98 | `RELATE-CONDITIONAL` | TSR's relater has no conditional-type arms of structuredTypeRelatedToWorker: (a) target conditional: related only if no infer params & not distribution-dependent (both branches); (b) source conditiona | internal/checker/relater.go:3540 (target Conditional arm) and :3721 (source Conditional arm) in structuredTypeRelatedToW | crates/tsr-checker/src/relater.rs (no conditional arm; relate_ternary) — consumers flow.rs narrowed_type_worker, declare | 2 | 0 | 3 |
| 99 | `RETURN-WIDEN-UNIQUE-SYMBOL` | Inferred return type of a function widens a unique symbol (getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded -> getWidenedUniqueESSymbolType) to symbol. | internal/checker/checker.go:20407 getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded, :25505 getWidenedUniqueESSym | crates/tsr-checker/src/signatures.rs get_return_type_from_body | 2 | 0 | 2 |
| 100 | `CONTEXTUAL-COMPUTED-SYMBOL-KEY` | getContextualTypeForObjectLiteralElement with a computed (unique symbol / late-bound) name passes the computed name type to getTypeOfPropertyOfContextualTypeEx, which for a generic mapped contextual t | internal/checker/checker.go:29920 getContextualTypeForObjectLiteralElement -> :30555 getTypeOfPropertyOfContextualTypeEx | crates/tsr-checker/src/contextual.rs contextual_type_for_object_literal_element (~L2125, computed names not resolved) | 1 | 0 | 20 |
| 101 | `NOINFER-SUBSTITUTION` | NoInfer<T> is an intrinsic alias creating a substitution type (getNoInferType); references strip it (getNarrowableTypeForReference), apparent type/property access and narrowing go through baseType. TS | internal/checker/checker.go:27394 getNoInferType; :31491 getNarrowableTypeForReference (isNoInferType strip) | crates/tsr-checker/src/(absent: no NoInfer reference in crates/tsr-checker/src) | 1 | 0 | 19 |
| 102 | `INTERSECTION-TYPEVAR-CONSTRAINT-REDUCTION` | getIntersectionTypeEx reduces T & P (T type variable, P primitive/literal-union/{}) using T's constraint: to T when constraint is a strict subtype of P, to never when unrelated. | internal/checker/checker.go:26130 getIntersectionTypeEx (constraint reduction block) | crates/tsr-checker/src/intersections.rs get_intersection_type | 1 | 0 | 19 |
| 103 | `APPARENT-MAPPED-ARRAY-CONSTRAINT` | Property access on a generic homomorphic mapped type whose type parameter is constrained to an array. Upstream's getResolvedApparentTypeOfMappedType maps over the constraint, so `.map` is Array<Promis | internal/checker/checker.go getApparentType -> getResolvedApparentTypeOfMappedType | crates/tsr-checker/src/members.rs (apparent type / property lookup of mapped types) | 1 | 0 | 15 |
| 104 | `FLOW-NONNULL-DECLARED-FALLBACK` | getFlowTypeOfReferenceEx: when the reference is the operand of `x!` and the flow type is non-never but entirely null/undefined (getTypeWithFacts NEUndefinedOrNull is never), the declared type is retur | internal/checker/flow.go:111 getFlowTypeOfReferenceEx | crates/tsr-checker/src/flow.rs get_flow_type_of_reference_ex (~L394) | 1 | 0 | 15 |
| 105 | `PATTERN-AMBIENT-MODULE` | resolveExternalModule's pattern-ambient-module arm: an import of `"foobarbaz"` matches `declare module "foo*baz"` via FindBestPatternMatch (longest prefix) and resolves to that module's symbol; import | internal/checker/checker.go:15149 resolveExternalModule (:15364-15366 patternAmbientModules / core.FindBestPatternMatch) | crates/tsr-checker/src (module resolution for imports: no pattern arm; check.rs has_pattern_ambient_module L13380 is dia | 1 | 0 | 13 |
| 106 | `OBJLIT-ACCESSOR-NONIDENT-NAME` | Object-literal get/set accessors with string/numeric literal names (paired via normalized symbol names) must be typed like identifier-named ones. | internal/checker/checker.go:13144 checkObjectLiteral accessor members (binder symbol naming via ast.GetPropertyNameForPr | crates/tsr-checker/src/objects.rs:1353 setter arm (only PropertyName::Identifier; else error) | 1 | 0 | 12 |
| 107 | `UNION-PROP-OBJECT-LITERAL-MISSING` | Reading a property from a union where an object-literal member lacks it yields an optional/undefined-typed union property. | internal/checker/checker.go createUnionOrIntersectionProperty (object-literal missing-prop arm) | crates/tsr-checker/src/members.rs union property lookup | 1 | 0 | 10 |
| 108 | `SPREAD-ELEMENT-EMPTY-TUPLE` | checkSpreadExpression on an empty tuple `[]` yields the iterated element type `never`; TSR array_spread_element_type has no answer for the zero-element tuple -> error. | internal/checker/checker.go checkSpreadExpression -> checkIteratedTypeOrElementType (getIteratedTypeOrElementType of []  | crates/tsr-checker/src/expressions.rs check_expression SpreadElement arm (~L999) / array_spread_element_type | 1 | 0 | 8 |
| 109 | `INTRA-EXPRESSION-INFERENCE-JSX-SPREAD` | Intra-expression inference for an object literal in a JSX spread attribute (`<Foo {...{ a: (x) => 10, b: (arg) => ... }} />`): the literal is contextually typed by the generic props type; context-inse | internal/checker/inference.go:1285 addIntraExpressionInferenceSite / :1302 inferFromIntraExpressionSites (JSX attrs: jsx | crates/tsr-checker/src/jsx*.rs / contextual.rs JSX spread-attribute contextual typing + signatures.rs get_type_of_functi | 1 | 0 | 8 |
| 110 | `BINDING-PATTERN-CONTEXT-LITERAL` | Object literal initializer contextually typed by the implied type of a destructuring pattern: properties with defaults become optional and keep literal types (literal contextual type from the default) | internal/checker/checker.go:13144 checkObjectLiteral (contextualTypeHasPattern arm ~13169/13252) with getTypeFromBinding | crates/tsr-checker/src/objects.rs:857 check_object_literal (no pattern-implied contextual type) | 1 | 0 | 8 |
| 111 | `CONTEXTUAL-DEFERRED-CONDITIONAL` | Object-literal argument contextually typed by a parameter that is a deferred conditional type (via its constraint/instantiated branches) so callback params get types. | internal/checker/checker.go getApparentTypeOfContextualType / instantiateContextualType | crates/tsr-checker/src/contextual.rs | 1 | 0 | 7 |
| 112 | `TUPLE-OPTIONAL-ELEMENT-UNDEFINED` | Optional tuple element types include undefined (addOptionality) and print as `(T \| undefined)?` under strictNullChecks. | internal/checker/checker.go:24205 getTypeFromOptionalTypeNode (addOptionality) + nodebuilder tuple printing | crates/tsr-checker/src/tuples.rs (optional element type stored without undefined) | 1 | 0 | 7 |
| 113 | `ARG-FINAL-CONTEXT-RECHECK` | Argument expression types in a generic call are those of isSignatureApplicable's final checkExpressionWithContextualType against the INSTANTIATED signature's parameter type (e.g. `[...H<[string, boole | internal/checker/checker.go:9256 isSignatureApplicable (checkExpressionWithContextualType) + :25522 isLiteralOfContextua | crates/tsr-checker/src/calls.rs choose_overload/argument recheck; contextual.rs contextual_type_for_argument (instantiat | 1 | 0 | 7 |
| 114 | `CONTEXTUAL-PROPERTY-OWN-MEMBERS` | getTypeOfPropertyOfContextualTypeEx maps union constituents through getTypeOfConcretePropertyOfContextualType (declared/resolved members only, then index infos); a function-type constituent contribute | internal/checker/checker.go:30555 getTypeOfPropertyOfContextualTypeEx (getTypeOfConcretePropertyOfContextualType) | crates/tsr-checker/src/contextual.rs union_contextual_property_type / contextual_type_for_object_literal_named_element ( | 1 | 0 | 6 |
| 115 | `FLOW-LOOP-INCOMPLETE-TYPES` | getTypeAtFlowLoopLabel: while a loop label is being analysed for a reference, re-entry (an assignment RHS in the loop that itself references the variable through a narrowing) returns the union of the  | internal/checker/flow.go:1325 getTypeAtFlowLoopLabel | crates/tsr-checker/src/flow.rs get_type_at_flow_loop_label (~L3018) | 1 | 0 | 6 |
| 116 | `PRINT-ALIAS-DEFAULT-TYPEARGS` | A reference to a generic alias written without args gets its defaults filled into alias.typeArguments, so it prints `ReactType<any>`. | internal/checker/checker.go:23580 getTypeFromTypeAliasReference (getTypeArgumentsFromNode fills defaults) | crates/tsr-checker/src/declared.rs qualified/alias reference minting (prints written text) | 1 | 0 | 5 |
| 117 | `DISCRIMINATE-CONTEXTUAL-BY-MEMBERS` | discriminateContextualTypeByObjectMembers discriminates a union contextual type using EVERY discriminant-capable member: property initializers via getContextFreeTypeOfExpression (identifiers like `kin | internal/checker/checker.go:30755 discriminateContextualTypeByObjectMembers (ObjectLiteralDiscriminator.matches :30730,  | crates/tsr-checker/src/symbols.rs discriminate_union_root (~L3223, literal-only discriminators) | 1 | 0 | 4 |
| 118 | `ARRAY-LITERAL-TUPLE-CONTEXT` | checkArrayLiteral's inTupleContext: contextual type (apparent type of a type parameter = its constraint, e.g. `readonly unknown[] \| []`) containing a tuple-like member makes `[]` a tuple `[]`; TSR yie | internal/checker/checker.go:8021 checkArrayLiteral (L8029 inTupleContext someType(isTupleLikeType\|\|generic homomorphic m | crates/tsr-checker/src/array_literals.rs (tuple-context decision with type-parameter contextual type) | 1 | 0 | 4 |
| 119 | `BINDER-ALIAS-EXPORT-CONTEXT` | declareModuleMember handles Alias symbols BEFORE the ExportContext test: an import-equals without `export` goes to the container's locals even inside an ambient (ExportContext) module/namespace. TSR a | internal/binder/binder.go:373 declareModuleMember (:376-381) | crates/tsr-binder/src/binder.rs is_exported_from_container (L2906-2918: ImportEqualsDeclaration arm gated to SourceFile  | 1 | 0 | 4 |
| 120 | `CONTEXTUAL-SIGNATURE-INTERSECTED` | When the contextual type has several applicable call signatures (overloaded console.log), getContextualCallSignature combines them via getIntersectedSignatures/combineSignaturesOfIntersectionMembers ( | internal/checker/checker.go:10305 getContextualCallSignature; :10314 getIntersectedSignatures | crates/tsr-checker/src/contextual.rs contextual_signature / signatures.rs get_type_of_function_expression | 1 | 0 | 4 |
| 121 | `THIS-ARG-INSTANTIATE-MEMBER` | Members of a class reference C<{}> are resolved with a mapper that also maps the polymorphic this type to the receiver; a member type `this extends C ? undefined : null` is instantiated and resolves. | internal/checker/checker.go:19106 resolveObjectTypeMembers (thisArgument mapper), :19095 resolveTypeReferenceMembers | crates/tsr-checker/src/members.rs get_type_of_property_with_this_argument (this not substituted into conditional types) | 1 | 0 | 4 |
| 122 | `LITERAL-CONTEXTUAL-INSTANTIABLE` | isLiteralOfContextualType keeps a literal fresh when the contextual type is (or contains) an instantiable type whose constraint admits literals — here the property type of Narrow<TNarrow> (conditional | internal/checker/checker.go:25522 isLiteralOfContextualType | crates/tsr-checker/src/literals.rs / contextual.rs (literal-context check through conditional\|mapped contextual types) | 1 | 0 | 4 |
| 123 | `INFERENCE-REVERSE-MAPPED-MEMBER` | Member types of a reverse-mapped inference (getTypeOfReverseMappedSymbol -> inferReverseMappedType) when the target is a nested/generic homomorphic mapped type (Boxified<Pick<T, K>>) or the mapped sou | internal/checker/inference.go:1066 inferReverseMappedType / :1145 getTypeOfReverseMappedSymbol (from inferToMappedType : | crates/tsr-checker/src/inference.rs reverse_mapped_member_type(_worker) (~L3275-3450) | 1 | 0 | 4 |
| 124 | `REVERSE-MAPPED-TUPLE` | Inference to a homomorphic mapped type from a tuple/array source builds a reverse-mapped tuple element-wise (incl. rest elements, optional->required for `?` modifier). | internal/checker/inference.go:1014 createReverseMappedType (tuple arm 1029) | crates/tsr-checker/src/inference.rs reverse-mapped inference (declines rest/variadic tuples) | 1 | 0 | 4 |
| 125 | `RETURN-LITERAL-OWN-SIG-CONTEXT` | getReturnTypeFromBody keeps a unit return type when the contextual signature IS the function's own signature (contextual type is a type parameter inferred from the function itself, e.g. ext<A>(a: A) w | internal/checker/checker.go:20209 getReturnTypeFromBody (contextualSignature == getSignatureFromDeclaration(fn) arm) | crates/tsr-checker/src/signatures.rs:2497 return_type_from_body (widens) | 1 | 0 | 3 |
| 126 | `CONDITIONAL-DISTRIBUTIVE-CONSTRAINT` | Constraint of a distributive conditional (Extract<M[K], ArrayLike<any>>) for narrowable references distributes over the check type's constraint. | internal/checker/checker.go:17284 getConstraintOfDistributiveConditionalType (via 31491 getNarrowableTypeForReference) | crates/tsr-checker/src/constraints.rs default_constraint_of_conditional_type | 1 | 0 | 3 |
| 127 | `LITERAL-OF-CONTEXTUAL-TYPEVAR` | Object-literal property literals under a GENERIC contextual type: checkExpressionForMutableLocation -> getWidenedLiteralLikeTypeForContextualType -> isLiteralOfContextualType with contextual type T["y | internal/checker/checker.go:25522 isLiteralOfContextualType (TypeVariable arm) via :25515 getWidenedLiteralLikeTypeForCo | crates/tsr-checker/src/signatures.rs is_literal_of_contextual_type (L3758) / object-literal property widening on the inf | 1 | 0 | 3 |
| 128 | `FLOW-IN-NONLITERAL-KEY` | `k in x` narrowing where the left operand is not a written string literal but an expression whose type is usable as a property name (e.g. const a = 'a'): upstream uses getTypeOfExpression(expr.Left) + | internal/checker/flow.go:530 narrowTypeByBinaryExpression InKeyword arm (getTypeOfExpression(expr.Left) -> narrowTypeByI | crates/tsr-checker/src/flow.rs narrow_type InKeyword arm (~L5080, written-literal boundary) -> narrow_type_by_in_keyword | 1 | 0 | 3 |
| 129 | `DESTRUCTURE-FLOW-DEPENDENT` | Destructured variables from a discriminated union narrow each other (dependent destructuring). Upstream's getFlowTypeOfDestructuring + discriminant narrowing of the sibling narrows p1 to number. TSR a | internal/checker/checker.go getFlowTypeOfDestructuring (called from :17707 getBindingElementTypeFromParentType); flow.go | crates/tsr-checker/src/destructure.rs (getFlowTypeOfDestructuring unported, module doc 'The named risk') | 1 | 0 | 3 |
| 130 | `ARG-CONTEXT-RESOLVED-SIG` | After a generic call resolves, re-checking an argument uses the parameter type of the RESOLVED (instantiated) signature as contextual type, so literals inside tuple-shaped args stay literal (isLiteral | internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex (reads getResolvedSignature) | crates/tsr-checker/src/contextual.rs:2348 contextual_type_for_argument (memo/uninstantiated roads) | 1 | 0 | 2 |
| 131 | `NARROW-INSTANCEOF-HASINSTANCE` | narrowTypeByInstanceof uses a right operand's [Symbol.hasInstance] method type predicate (`value is ...`) to narrow; TSR narrows by construct-signature instance type only | internal/checker/checker.go narrowTypeByInstanceof (getSymbolHasInstanceMethodOfObjectType -> type predicate) | crates/tsr-checker/src/flow.rs instanceof narrowing | 1 | 0 | 2 |
| 132 | `FLOW-IN-RECORD-INTERSECT` | `"p" in x` where no constituent of x declares p: upstream narrowTypeByInKeyword (assumeTrue) intersects with Record<"p", unknown> via the global Record alias; TSR returns t unchanged (Record alias ins | internal/checker/flow.go:1001 narrowTypeByInKeyword (Record intersection arm :1014) | crates/tsr-checker/src/flow.rs narrow_type_by_in_keyword (`if !known { return t; }`) | 1 | 0 | 2 |
| 133 | `INFERENCE-REVERSE-MAPPED-INTERSECTION` | Inference to a homomorphic mapped type `{[K in keyof T]: T[K]}` from an INTERSECTION source ({a}&{b}): inferToMappedType -> inferTypeForHomomorphicMappedType -> createReverseMappedType over the inters | internal/checker/inference.go:948 inferToMappedType / :1004 inferTypeForHomomorphicMappedType / :1014 createReverseMappe | crates/tsr-checker/src/inference.rs reverse_mapped_member_plan / reverse_mapped_member_type (~L3275-3570) | 1 | 0 | 2 |
| 134 | `TUPLE-OPTIONAL-ELEMENT-OPTIONALITY` | Optional tuple element types get addOptionality (`string?` stores string\|undefined and prints `(string \| undefined)?`). | internal/checker/checker.go:24205 getTypeFromOptionalTypeNode (also 24164 named members) | crates/tsr-checker/src/declared.rs tuple type-node arm (TupleTypeNode / OptionalType element) | 1 | 0 | 2 |
| 135 | `FLOW-ASSIGNMENT-REDUCED-GENERIC` | getAssignmentReducedType filters a declared union by typeMaybeAssignableTo(assigned, constituent); for generic interface instantiations (Some<r> vs None, {some: r} vs Some<r>) upstream decides structu | internal/checker/flow.go:2399 getAssignmentReducedType (from flow.go:220 getTypeAtFlowAssignment) | crates/tsr-checker/src/flow.rs get_assignment_reduced_type (~L2496) | 1 | 0 | 2 |
| 136 | `OBJLIT-PROPERTY-NAME-CONTEXTUAL-LITERAL` | The declaration-name line of an object-literal PropertyAssignment is getTypeOfSymbol -> checkPropertyAssignment -> checkExpressionForMutableLocation, which keeps a literal when the contextual type adm | internal/checker/checker.go:16611 getTypeOfVariableOrParameterOrPropertyWorker (PropertyAssignment) -> checker.go:13673  | crates/tsr-checker/src/objects.rs member symbol type recorded via symbol_types.entry().or_insert (widened, context-free) | 1 | 0 | 2 |
| 137 | `AWAITED-THENABLE-EVAL` | Awaited<Promise3<...>> over a jQuery-style 12-type-param overloaded `then` evaluates to string; TSR errors. Single-overload reductions with few type params agree, so the trigger is in the full overloa | internal/checker/checker.go:24300 getConditionalType / inference.go:838 inferFromSignatures | crates/tsr-checker/src/declared.rs evaluate_conditional_inference (7252) | 1 | 0 | 1 |
| 138 | `INFERENCE-GENERIC-SOURCE-SIG` | Inferring from a GENERIC source signature (e.g. Boolean `<T>(v?: T) => boolean`) to a type-predicate overload `<S extends T>(p: (v) => v is S)` must use getBaseSignature and let overload resolution pr | internal/checker/inference.go:838 inferFromSignatures; checker.go:19438 getBaseSignature | crates/tsr-checker/src/inference.rs infer_from_types_with_priority (signature arm) | 1 | 0 | 1 |
| 139 | `SHORTHAND-PROPERTY-CONTEXTUAL-LITERAL` | checkShorthandPropertyAssignment -> checkExpressionForMutableLocation(name) keeps the identifier's literal type when the contextual type contains that literal (getWidenedLiteralLikeTypeForContextualTy | internal/checker/checker.go:13229 checkShorthandPropertyAssignment -> :13878 checkExpressionForMutableLocation | crates/tsr-checker/src/objects.rs / expressions.rs shorthand property member typing (widens unconditionally) | 1 | 0 | 1 |
| 140 | `RELATER-NONPRIMITIVE-INDEX-SIGNATURE` | `object` source vs target with string index signature is decidably not related (no implicit index signature), so overload resolution falls through. | internal/checker/relater.go:4603 typeRelatedToIndexInfo / 4578 indexSignaturesRelatedTo | crates/tsr-checker/src/relater.rs (NON_PRIMITIVE source arms ~L1725; undecided -> error) | 1 | 0 | 1 |
| 141 | `NARROWABLE-REF-INFERENTIAL-GATE` | getNarrowableTypeForReference skips constraint substitution only under CheckModeInferential; the final (non-inferential) argument check of a generic call still substitutes (k: K extends keyof S passed | internal/checker/checker.go:31491 getNarrowableTypeForReference (checkMode&CheckModeInferential gate) | crates/tsr-checker/src/constraints.rs narrowable_type_for_reference (`if !self.active_inference_contexts.is_empty() { re | 1 | 0 | 1 |
| 142 | `INDEXED-ACCESS-NEVER-INDEX` | Indexed access with a never index (`object[keyof object]`) is never. | internal/checker/checker.go:27001 getPropertyTypeForIndexType (via 26935 getIndexedAccessTypeOrUndefined) | crates/tsr-checker/src/indexed access evaluation | 1 | 0 | 1 |
| 143 | `JSX-GENERIC-COMPONENT-INFERENCE` | JSX opening element on a generic class component infers type arguments (inferJsxTypeArguments); attributes then get literal contextual types. | internal/checker/jsx.go:197 inferJsxTypeArguments / 544 resolveJsxOpeningLikeElement | crates/tsr-checker/src/jsx resolution for generic components | 1 | 0 | 1 |
| 144 | `JSX-ATTR-CONTEXTUAL-LITERAL` | checkJsxAttribute uses checkExpressionForMutableLocation; for a class component whose props are `Readonly<P & {...}>` with generic P, the attribute's contextual type is a generic indexed access so isL | internal/checker/jsx.go:871 checkJsxAttribute; checker.go:25522 isLiteralOfContextualType | crates/tsr-checker/src/jsx_intrinsic.rs jsx_attribute_context (L723) / jsx_attributes_context (L92) | 1 | 0 | 1 |
| 145 | `GENERIC-REDUCIBLE-INDEXED-ACCESS` | An indexed access whose object is a reducible intersection with a generic discriminant (`(Payload & { dataType: K })["data"]`) is a generic object type (ObjectFlagsIsGenericObjectType via isGenericRed | internal/checker/checker.go:24880 getGenericObjectFlags; :24932 isGenericReducibleType; :24937 isReducibleIntersection;  | crates/tsr-checker/src (indexed access resolution; get_indexed_access_type) | 1 | 0 | 1 |
| 146 | `DECLNAME-LIB-MERGED-SYMBOL` | The declaration-name line of a member declared in a user interface that merges with a lib interface (`interface Console { log(...) }`) types the MERGED symbol (getSymbolOfDeclaration → getMergedSymbol | internal/checker/checker.go:31991 getTypeOfNode IsDeclaration arm → :14390 getSymbolOfDeclaration → :14355 getMergedSymb | crates/tsr-conformance/src/types_producer.rs declaration-name arm / crates/tsr-checker/src/symbols.rs get_type_of_symbol | 1 | 0 | 1 |
| 147 | `PRINT-TYPE-PRECEDENCE` | Printer parenthesises union constituents by type precedence (conditional/function types inside a union get parens). | internal/printer/printer.go:2038 emitUnionTypeConstituent (emitTypeNode with TypePrecedenceTypeOperator) | crates/tsr-checker/src/printing.rs union text (no parens for conditional constituents) | 1 | 0 | 1 |
| 148 | `INDEXED-ACCESS-GENERIC-DEFER` | Element access whose index type is generic (T, D \| (T & number)) yields a deferred indexed access type `(typeof D)[T]` (isGenericIndexType -> deferred) even when the object is an enum object with a nu | internal/checker/checker.go:26935 getIndexedAccessTypeOrUndefined (isGenericIndexType checker.go:24876) | crates/tsr-checker/src/indexed.rs check_element_access_type / deferred_indexed_access (enum-object path resolves before  | 1 | 0 | 1 |
| 149 | `OBJLIT-PROPERTY-SYMBOL-TYPE` | The property-assignment NAME line of an object literal reads getTypeOfSymbol(prop), which for an object-literal member is the type checkObjectLiteral recorded during the (contextual) check - literal p | internal/checker/checker.go:13144 checkObjectLiteral (member symbol links.resolvedType) + getWidenedLiteralLikeTypeForCo | crates/tsr-checker/src/objects.rs check_object_literal symbol_types cache (§892/§890 exclusion of call arguments) / symb | 1 | 0 | 1 |
| 150 | `MODULE-INSTANCE-STATE-EXPORT-ALIAS` | getModuleInstanceState for an ExportDeclaration/export specifier must follow getModuleInstanceStateForAliasTarget (type-only target => NonInstantiated). Drives writer's value-meaning skip of namespace | internal/ast/utilities.go:2352 getModuleInstanceStateWorker / utilities.go:2406 getModuleInstanceStateForAliasTarget | crates/tsr-ast/src/lib.rs module_instance_state_at (ExportDeclaration falls to `_ => Instantiated`) | 1 | 0 | 1 |
| 151 | `INFERENCE-PRIMITIVE-CONSTRAINT-REGULAR` | getCovariantInference: with a primitive constraint, candidates are mapped through getRegularTypeOfLiteralType (which maps union constituents too), so the inferred union of literals is non-fresh and a  | internal/checker/inference.go:1434 getCovariantInference (primitiveConstraint -> getRegularTypeOfLiteralType) | crates/tsr-checker/src/inference.rs inferred-type candidate handling | 1 | 0 | 1 |
| 152 | `TEMPLATE-SPAN-CONSTRAINT-ASSIGNABILITY` | checkTemplateExpression keeps each span's type if isTypeAssignableTo(t, templateConstraintType) else string. For a type parameter constrained by an alias instantiation (`Keyof<R>` = `keyof T & string` | internal/checker/checker.go:7976 checkTemplateExpression (templateConstraintType test, +13) | crates/tsr-checker/src/templates.rs template expression span typing / relater.rs type-parameter constraint of alias-inst | 1 | 0 | 1 |

## Verified cluster details

### 1. `CLASS-GET-BASE-TYPES` (V; blocked 45, finished alone 23, lines 71; confidence high)

- **Root cause:** The writer's heritage line is GetTypeAtLocation(EWTA) = getTypeOfNode's class-extends arm = getTypeWithThisArgument(getBaseTypes(classType)[0]) (errorType -> writer falls back to the expression's type); TSR has no getBaseTypes/resolveBaseTypesOfClass/getBaseConstructorTypeOfClass and instead re-derives the base syntactically in two producer arms (heritage_base_symbol + base_type_of_heritage_entry; resolve_name + heritage_reaches + qualified_heritage_reference), which cannot type expression bases, ignores the first-entry rule, skips arity validation/default filling, misses qualified/generic cycles, ignores merged-interface bases and mints written-text names.
- **tsgo:** internal/checker/checker.go:31959 getTypeOfNode (class-extends EWTA arm) -> :19167 getBaseTypes -> :19220 resolveBaseTypesOfClass -> :16957 getBaseConstructorTypeOfClass (+ :19275 getInstantiatedConstructorsForTypeArguments, :23169 getTypeFromClassOrInterfaceReference, :19498 resolveBaseTypesOfInterface); writer internal/testutil/tsbaseline/type_symbol_baseline.go:372
- **TSR:** crates/tsr-conformance/src/types_producer.rs:752 type_id_at_location_tracking heritage arm 1 (heritage_base_symbol :263, declared.rs:149 base_type_of_heritage_entry) and :881 heritage arm 2 (heritage_reaches :312, declared.rs:4566 qualified_heritage_reference); new Checker::get_base_types to add in crates/tsr-checker (no such function exists today)
- **Examples:**
  - `compiler/declarationEmitExpressionInExtends2`: `getClass(2)`. want `C<string, number>`, got `typeof C`. `class MyClass extends getClass(2)<string, number>`: getBaseConstructorTypeOfClass checks the call (typeof C), C is a class with all outer params applied, so resolveBaseTypesOfClass = getTypeFromClassOrInterfaceReference(EWTA) = C<string, number>. TSR's heritage_base_symbol only follows Identifier/PropertyAccess names, both producer arms decline, and the expression type typeof C is printed.
  - `compiler/classExtendsMultipleBaseClasses`: `B`. want `A`, got `B`. `class C extends A,B`: getBaseTypeNodeOfClass takes only the first extends element, so getTypeOfNode(EWTA B) still answers getBaseTypes(C)[0] = A. TSR producer arm 2 resolves each entry's own expression and prints B.
  - `compiler/superCallFromClassThatDerivesNonGenericTypeButWithTypeArguments1`: `A`. want `typeof A`, got `A<number, string>`. `class B extends A<number, string>` with non-generic A: getTypeFromClassOrInterfaceReference reports arity and returns errorType, resolveBaseTypesOfClass leaves no base, getTypeOfNode returns errorType and the writer falls back to typeof A. TSR arm 1 calls base_type_of_heritage_entry = create_type_reference(A,[number,string]) with no arity check. Probe h.ts confirms the family: `extends K<string>`
- **Port plan:** Port getBaseConstructorTypeOfClass (check_expression of getEffectiveBaseTypeNode's expression, memoized per class symbol with a resolution-stack circularity guard -> errorType; non-constructor non-any -> errorType), resolveBaseTypesOfClass (getApparentType of it; Class-symbol branch with areAllOuterTypeParametersApplied -> getTypeFromClassOrInterfaceReference semantics = arity window + fillMissingTypeArguments incl. JS any defaults — reuse declared.rs:38 instantiated_heritage_base which already does this; any -> any; else getInstantiatedConstructorsForTypeArguments -> return type of first instantiated construct signature — reuse the signature loop in assignment_declarations.rs:280 instance_base_type_of_heritage_entry; then isValidBaseType and the t==base||hasBaseType cycle check), resolveBaseTypesOfInterface for interface declarations merged with the class, and getBaseTypes (memo + pushTypeResolution) as one cached Checker::get_base_types(class_symbol). Then replace BOTH producer heritage arms (types_producer.rs:752-781 and :881-1138) with: if parent is EWTA in a class extends clause, t = get_base_types(owner class)[0] with this-arg (getTypeWithThisArgument; for a class reference it prints the same), and if none or any fall through to the ordinary expression road — exactly the writer's `t == nil || IsTypeAny(t)`. Delete heritage_base_symbol, heritage_reaches, is_ewta guards' symbol filters, Checker::base_type_of_heritage_entry and qualified_heritage_reference (and their tests in tsr-checker/tests/heritage_base_type.rs) — the normal type printer already prints `D` inside Module (oracle rb.ts). Pitfalls: base_symbols_of / check_super_expression / static walk keep their own syntactic resolution until clusters MEMBERS/SUPER/STATIC migrate, so do not route them through the new op in the same change unless measured; arm 2 currently answers some cycles and value-shadowing (§479 typeValueConflict, §475 type-parameter base, §837 cycles) by hand — those all fall out of getBaseConstructorTypeOfClass (check_expression value meaning) + cycle check and must be re-measured. Some lines also need another fix to flip: interfaceExtendsObjectIntersection (TSR types the merged `Constructor<I1>()` as any -> TYPE-PARAMS-ALL-DECLS), accessorsOverrideProperty8 (expression type wrong -> LITERAL-OF-CONTEXTUAL-TYPEVAR), jsxInExtendsClause (call errors -> G17 INFER-NO-CANDIDATE, verified `f4<P>(x:{p?:P}); f4({})` TSR error / tsgo unknown), jsxCallbackWithDestructuring (interface-declared defaults -> TYPE-PARAMS-ALL-DECLS).
- **Finished alone (23):** compiler/classDeclaredBeforeClassFactory, compiler/classExtendsMultipleBaseClasses, compiler/declFileWithClassNameConflictingWithClassReferredByExtendsClause, compiler/declarationEmitExpressionInExtends2, compiler/declarationEmitExpressionInExtends6, compiler/declarationEmitForDefaultExportClassExtendingExpression01, compiler/declareDottedExtend, compiler/defaultPropsEmptyCurlyBecomesAnyForJs, compiler/extendNonClassSymbol1, compiler/extendsClauseAlreadySeen2, compiler/jsxInExtendsClause, compiler/recursiveBaseCheck, compiler/recursiveBaseCheck2, compiler/recursiveClassBaseType, compiler/subclassUint8Array, compiler/tsxFragmentChildrenCheck, compiler/useBeforeDeclaration_propertyAssignment, conformance/awaitClassExpression_es2017, conformance/awaitClassExpression_es6, conformance/classExtendsItselfIndirectly2, conformance/emitClassDeclarationWithPropertyAccessInHeritageClause1, conformance/extendClassExpressionFromModule, conformance/mixinAccessors5
- **Also blocked (other clusters needed) (22):** compiler/anonClassDeclarationEmitIsAnon (+CLASS-TYPE-BASE-TYPE-VARIABLE-INTERSECTION); compiler/declarationEmitClassMixinLocalClassDeclaration (+UNATTRIBUTED); compiler/declarationEmitExpressionInExtends (+CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES); compiler/exportDefaultAbstractClass (+RESOLVE-NAME-EXPORT-DEFAULT-LOCAL); compiler/interfaceMergeWithNonGenericTypeArguments (+SUPER-FROM-BASE-TYPES); compiler/jsDeclarationEmitDoesNotRenameImport (+IMPORT-TYPE-NODE-TYPEOF); compiler/jsxCallbackWithDestructuring (+APPEND-LOCAL-TYPE-PARAMETERS); compiler/jsxComplexSignatureHasApplicabilityError (+QUALIFIED-NAME-LEFT-ALIAS-RESOLVE, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, unaligned); compiler/jsxHasLiteralType (+JSX-GENERIC-COMPONENT-INFERENCE); compiler/missingPropertiesOfClassExpression (+SUPER-FROM-BASE-TYPES); compiler/reactHOCSpreadprops (+QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER); compiler/reactReadonlyHOCAssignabilityReal (+JSX-ATTR-CONTEXTUAL-LITERAL, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER); compiler/superCallFromClassThatDerivesFromGenericTypeButWithIncorrectNumberOfTypeArguments1 (+SUPER-FROM-BASE-TYPES, UNTYPED-CALL-ARGUMENT-ANY-CONTEXT); compiler/superCallFromClassThatDerivesNonGenericTypeButWithTypeArguments1 (+UNTYPED-CALL-ARGUMENT-ANY-CONTEXT); compiler/unusedInvalidTypeArguments (+TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL); conformance/accessorsOverrideProperty8 (+LITERAL-OF-CONTEXTUAL-TYPEVAR); conformance/classExpression3 (+CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES); conformance/classExpressionES63 (+CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES); conformance/importCallExpression3ES2020 (+CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES); conformance/importCallExpressionInCJS4 (+CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES); conformance/interfaceExtendsObjectIntersection (+APPEND-LOCAL-TYPE-PARAMETERS, DEFERRED-TYPE-REFERENCE-ALIAS); conformance/mixinClassesMembers (+CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES, SUPER-FROM-BASE-TYPES)

### 2. `UNTYPED-CALL-ANY-CALLEE` (V; blocked 16, finished alone 16, lines 35; confidence high)

- **Root cause:** resolveCallExpression/resolveNewExpression treat ANY callee type (IsTypeAny(funcType), and resolveNewExpression's IsTypeAny(apparent)) as an untyped call answering anySignature->any; TSR's is_untyped_call_target admits only a source-WRITTEN any (annotation/cast/unresolved name/any receiver), so inferred/implicit/member/element any callees gap.
- **tsgo:** internal/checker/checker.go:9933 isUntypedFunctionCall (IsTypeAny(funcType) disjunct, called from resolveCallExpression checker.go:8529; new: resolveNewExpression checker.go:8593 IsTypeAny arm; both -> resolveUntypedCall checker.go:9902)
- **TSR:** crates/tsr-checker/src/calls.rs:2113 is_untyped_call_target (+ any_is_written_in_an_annotation calls.rs:2218; callers calls.rs:683 call arm, expressions.rs:2475 new arm, calls.rs:1195 tag arm)
- **Examples:**
  - `compiler/castParentheses`: `a.b()`. want `any`, got `error`. `static b: any` makes the callee a property access typed any; tsgo isUntypedFunctionCall IsTypeAny -> resolveUntypedCall -> any (no .errors.txt, so a real any). TSR: callee is not an identifier/cast and receiver `typeof a` is not any, so the provenance gate refuses and resolve_call_signature gaps (verdictdump 0:24 GAP; probe `declare var o:{b:any}; o.b()` also error).
  - `compiler/localClassesInLoop`: `data[0]()`. want `any`, got `error`. `var data = []` (strict:false) widens to any[], so data[0] is any; tsgo untyped call -> any. TSR prints `data[0] : any` RIGHT but the element-access callee has no written any and receiver any[] is not intrinsic any, so the gate refuses (probe: d[0](), t[0]() on [any] and o.zz() via any index signature all error).
  - `compiler/noCollisionThisExpressionAndLocalVarInProperty`: `callback(_this)`. want `any`, got `error`. `doStuff: (callback) => () => callback(_this)` sits in an unannotated class-property initializer, so there is no contextual type and callback is implicit any; tsgo untyped call -> any. TSR's documented 'positional refusal' rejects every unannotated parameter even when has_no_contextual_type holds (probe `var obj = {doStuff: (callback) => () => callback(1)}` and `function h(cb){return cb(1)}` both 
- **Port plan:** Upstream's test is just IsTypeAny(funcType) (call) / IsTypeAny(apparentType) after isErrorType (new). TSR cannot adopt it verbatim because its any is not yet trustworthy (calls.rs:2127 records 248 gap->wrong when it did), so widen the provenance gate by the provenances these cases prove honest, each a separate measurable arm in any_is_written_in_an_annotation/is_untyped_call_target: (1) identifier resolving to an unannotated PARAMETER whose function has_no_contextual_type (signatures.rs:4406) - replaces the blanket positional refusal (collisionThis*/noCollisionThis*, 8 cases but ONE shape); (2) property/element access whose resolved member/index-info/element declared type node is the `any` keyword or whose receiver is an array/tuple of written-or-widened any (castParentheses static b:any, localClassesInLoop data[0]); (3) widened implicit-any from null/undefined initializers or getter `return null` under strict:false (typeCheckObjectCreation... `var classes = undefined`, classPropertyIsPublicByDefault `static get b(){return null}`); (4) globalThis missing property (checker.go:11337-11344 returns anyType: wrappedIncovations1/2 `this.foo()`). Also variableDeclaratorResolvedDuringContextualTyping: `this.jsonToStat(...)` on a class `this` is upstream errorType -> resolveErrorCall (checker.go:8516/9923, unknownSignature -> errorType printed any); TSR already answers that missing member as `any` (probe `{s:this.missing}` RIGHT) so the same gate change makes the call any and the enclosing literal `{stat:any;isNew:boolean}` stop collapsing - porter should return the same value TSR uses for the missing member, not gap. Apply the same gate in expressions.rs:2475 (new). Pitfall: keep error (gap) refused; measure each arm separately; long-term goal is deleting the provenance machinery once implicit-any params are contextually typed.
- **Finished alone (16):** compiler/castParentheses, compiler/collisionThisExpressionAndLocalVarInConstructor, compiler/collisionThisExpressionAndLocalVarInMethod, compiler/collisionThisExpressionAndLocalVarInProperty, compiler/collisionThisExpressionAndParameter, compiler/localClassesInLoop, compiler/localClassesInLoop_ES6, compiler/noCollisionThisExpressionAndLocalVarInAccessors, compiler/noCollisionThisExpressionAndLocalVarInConstructor, compiler/noCollisionThisExpressionAndLocalVarInMethod, compiler/noCollisionThisExpressionAndLocalVarInProperty, compiler/typeCheckObjectCreationExpressionWithUndefinedCallResolutionData, compiler/variableDeclaratorResolvedDuringContextualTyping, compiler/wrappedIncovations1, compiler/wrappedIncovations2, conformance/classPropertyIsPublicByDefault

### 3. `TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION` (V; blocked 30, finished alone 15, lines 106; confidence high)

- **Root cause:** TSR has no TypeAlias attribute on types: a generic alias's declared type is minted as an opaque nominal `Name<Params>` (declared.rs:6048) and every reference is an opaque (symbol,args) pair via create_type_reference, with a whitelist of syntactic body shapes re-walked under alias_evaluation_bindings, instead of upstream's declared type = the body type as built (alias attached only by alias-accepting constructors) and reference = instantiateTypeWithAlias(body, mapper, newAlias), which drops/replaces the alias whenever the body is or reduces to a pre-existing type.
- **tsgo:** internal/checker/checker.go:23837 getDeclaredTypeOfTypeAlias; :23641 getTypeAliasInstantiation -> :22104 instantiateTypeWithAlias (alias sources: :23719 getAliasSymbolForTypeNode; drops: :26127 getIntersectionTypeEx len(typeSet)==1, :25632 getUnionTypeEx len(types)==1, type-parameter body returns mapped arg)
- **TSR:** crates/tsr-checker/src/declared.rs:5963 get_declared_type_of_type_alias (mint at :6048) + crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference (opaque create_type_reference at :4377); data model crates/tsr-checker/src/types.rs TypeData (no alias field)
- **Examples:**
  - `compiler/conditionalTypeAnyUnion`: `R`. want `any`, got `WithSpec<any>`. `type WithSpec<T extends number> = T; type R = WithSpec<Spec>` with Spec=any: getTypeAliasInstantiation instantiates the bare type-parameter body, which returns the argument itself and ignores the alias. TSR interns (WithSpec,[any]) and prints the pair (oracle: `type Id<T>=T; type X1=Id<string>` tsgo `string`, TSR `Id<string>`).
  - `conformance/templateLiteralTypes8`: `Stringify`. want ``${T}``, got `Stringify<T>`. Declared type of `type Stringify<T extends string> = `${T}`` is getTypeFromTemplateTypeNode's interned template type, which never carries an alias. TSR's generic-alias arm mints `Stringify<T>` regardless of body (same for typeof/keyof bodies; oracle p1: S/K/VA).
  - `conformance/intersectionsAndEmptyObjects`: `defaultChoices`. want `{ shoes: boolean; food: boolean; }`, got `choices<{}>`. `choices<{}>` instantiates the body `IChoiceList & {shoes;food}`; getIntersectionTypeEx removes the redundant `{}` supertype and returns the single remaining member before any alias is attached. TSR never instantiates the body and prints the (alias,args) reference (oracle: `Ch<{}>` -> `{ shoes: boolean; }`).
- **Port plan:** Prerequisite: give TSR types an upstream-shaped alias attribute (alias symbol + alias type arguments) that the printer prefers over structure, set only by the constructors that receive getAliasForTypeNode (object literal, union, intersection, mapped, conditional, indexed access, type-reference instantiation) and instantiated by instantiate_type (instantiateTypeAlias). Then (1) replace the generic-alias mint at declared.rs:6048 and its special-case arms (keyword body :5981, bare type-param :5997, rest-tuple :6025, mapped-sequence :6039) with: resolve the body via get_type_from_type_node and store type parameters + instantiation cache keyed by the parameters (getDeclaredTypeOfTypeAlias). (2) Make get_instantiated_type_reference for TYPE_ALIAS symbols call a ported getTypeAliasInstantiation: build the mapper (fillMissingTypeArguments already exists), compute newAlias per getTypeFromTypeAliasReference:23580 (alias-decl host, local-alias rule, import-alias target), and instantiateTypeWithAlias(declared, mapper, newAlias); delete the conditional/function/variadic/identity-mapped special arms (:4163-:4390) and evaluate_alias_body/evaluate_conditional_alias road once instantiate_type covers conditional/indexed-access/mapped/keyof. Pitfalls: intersection/union constructors must return the singleton BEFORE attaching the alias (getIntersectionTypeEx :26127, getUnionTypeEx :25632); type-parameter bodies return the image untouched; non-generic conditional bodies resolve at declaration (nestedGenericConditional T<X> -> any); recursive aliases (Tree<T>, C1<T>) rely on deferred resolution, so the declared body must not be eagerly expanded; huge RIGHT surface currently depends on the mint (Box<T>, Fn<T>, Un<T> print their alias in tsgo too), so measure RIGHT->WRONG.
- **Finished alone (15):** compiler/aliasInstantiationExpressionGenericIntersectionNoCrash2, compiler/conditionalTypeAnyUnion, compiler/declarationEmitNoInvalidCommentReuse3, compiler/deferredLookupTypeResolution, compiler/deferredLookupTypeResolution2, compiler/intersectionApparentTypeCaching, compiler/mappedTypeAndIndexSignatureRelation [type-operators], compiler/nestedGenericConditionalTypeWithGenericImportType, compiler/recursiveConditionalCrash1, compiler/recursiveConditionalCrash2, compiler/recursiveTypeAliasWithSpreadConditionalReturnNotCircular, compiler/templateLiteralIntersection [type-operators], conformance/numericStringLiteralTypes [type-operators], conformance/templateLiteralTypes8 [type-operators], conformance/unknownType2
- **Also blocked (other clusters needed) (15):** compiler/aliasOfGenericFunctionWithRestBehavedSameAsUnaliased (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/coAndContraVariantInferences3 (+CONDITIONAL-TYPE-ROOT-ALIAS); compiler/computedTypesKeyofNoIndexSignatureType (+CONDITIONAL-GENERIC-SIGNATURE-EXTENDS, INSTANTIATE-MAPPED-TYPE-ALIAS-DROP); compiler/contextualTypeBasedOnIntersectionWithAnyInTheMix5 (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, unaligned); compiler/declarationEmitMappedTypePreservesTypeParameterConstraint [type-operators] (+DEFERRED-TYPE-REFERENCE-ALIAS, INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, RELATER-NONPRIMITIVE-INDEX-SIGNATURE, SIGNATURE-DECLARATION-ANNOTATION-REUSE, unaligned); compiler/declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1 (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); compiler/declarationEmitOptionalMappedTypePropertyNoStrictNullChecks2 (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); compiler/declarationEmitOptionalMappedTypePropertyNoStrictNullChecks3 (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); compiler/deeplyNestedMappedTypes (+CONDITIONAL-TYPE-ROOT-ALIAS, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/indexingTypesWithNever (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, unaligned); compiler/intersectionConstraintReduction (+, unaligned); compiler/mappedTypeGenericIndexedAccess [type-operators] (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); compiler/typePredicateFreshLiteralWidening (+LITERAL-CONTEXTUAL-INSTANTIABLE); conformance/intersectionsAndEmptyObjects (+IMPORT-CALL-SYNTHETIC-DEFAULT-TYPE); conformance/keyofIntersection (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS)

### 4. `TYPE-ALIAS-INSTANTIATION-NEW-ALIAS` (V; blocked 29, finished alone 13, lines 93; confidence high)

- **Root cause:** getTypeFromTypeAliasReference's newAliasSymbol arm (outer non-local alias whose body is directly a generic-alias reference) is not ported: TSR instantiates the inner alias as an alias-blind named reference `Inner<args>` instead of instantiateTypeWithAlias(body, mapper, Outer).
- **tsgo:** internal/checker/checker.go:23580 getTypeFromTypeAliasReference (newAliasSymbol 23609-23616) -> :23641 getTypeAliasInstantiation -> :22104 instantiateTypeWithAlias -> :22220 instantiateTypeWorker (alias arms) / :22535 instantiateMappedType
- **TSR:** crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference (final create_type_reference arm at :4377; cache in declared.rs:5157 create_type_reference_with_display keyed (symbol,args) with no alias)
- **Examples:**
  - `compiler/omitTypeTests01`: `Bar (0:3)`. want `Bar`, got `Omit<Foo, "c">`. `export type Bar = Omit<Foo, "c">`: Omit's declared type is a Pick (non-homomorphic mapped) instantiation, so getObjectTypeInstantiation/instantiateMappedType->instantiateAnonymousType take newAlias Bar. TSR create_type_reference mints the text `Omit<Foo, "c">`; downstream bar/baz lines follow.
  - `conformance/intersectionTypeInference3`: `A (0:5)`. want `A`, got `Nominal<"A", string>`. `type A = Nominal<'A', string>` with `Nominal<K,T> = T & {...}`: instantiateTypeWorker's union/intersection arm (22258) calls getIntersectionTypeEx(..., alias A). TSR keeps the inner alias; Set<A>/A[] lines downstream inherit it.
  - `conformance/mappedTypeIndexSignatureModifiers`: `Res1 (0:1)`. want `Res1`, got `Pick<Obj1, keyof Obj1>`. `type Res1 = Pick<Obj1, keyof Obj1>` re-aliases (non-homomorphic mapped). Same file's control: `type Res6 = Identity<Obj6>` (homomorphic `{[P in keyof T]: T[P]}`) prints `Identity<Obj6>` upstream and TSR is RIGHT there - instantiateMappedType's constituent path (22565) passes nil alias. Oracle probe: `type A12 = Partial<{a:number}>` -> `Partial<{ a: number; }>`, `type A1 = Box<string>` -> `A1`, `t
- **Port plan:** In get_instantiated_type_reference, for a TYPE_ALIAS target compute newAlias = alias_symbol_for_type_node(node) (TSR's helper already folds upstream's isLocalTypeAlias refusal: it returns None for function/block-local outer aliases, which upstream either refuses (23611) or prints structurally) and only when the outer alias has no type parameters (a generic outer alias is minted nominally by get_declared_type_of_type_alias:6048 and never reaches here). Thread it into the instantiation as upstream does: the instantiation cache key must include the alias (getTypeAliasInstantiationKey), and the alias-named mint must keep everything consumers read from the inner result - type_reference_targets=(symbol,args), member owner (alias_body_literal_symbol / conditional branch symbol), signature_types + alias_named_signature_types for function/constructor bodies (generalise §947.2 at :4239), intersection/union operands, mapped_identity_sources. Decide alias landing by porting instantiateTypeWorker's arms, not by syntax guesses: object literal / function / non-homomorphic mapped (Pick, Record, Omit, Clone with `keyof (T & {})`) / union / intersection / homomorphic-mapped over a UNION (mapTypeWithAlias) / distributive conditional -> outer alias; homomorphic mapped over a non-union (Partial, Readonly, Identity), keyword bodies, resolved conditional branch, indexed access distributing to pre-existing types (oracle: `Idx<{a:'x'}|{a:'y'}>` -> "x" | "y") -> no outer alias. Then delete the two special cases that already approximate this arm: the mapped-union re-alias at declared.rs:4381-4390 and the alias argument passed to evaluate_conditional_alias at :4199-4206 become the general path. Pitfalls: keyofIntersection T05/T06/T07/Result1/Result5 need the body evaluated to a union first (keyof (T & B) declares as `"b" | keyof T`) - depends on G04 ALIAS-BODY-INSTANTIATION; correlatedUnions 0:442/0:443 also need the type-parameter constraint printed by node reuse (`K extends KeyOfOriginal`, nodebuilderimpl.go:1615) - a second blocker on the same lines; recursiveGenericUnionType1/2 and varianceProbling.../...Align2 and symbolLink...ModuleNames/...RootDir are near-duplicate pairs, not independent witnesses.
- **Finished alone (13):** compiler/caseInsensitiveFileSystemWithCapsImportTypeDeclarations, compiler/eventEmitterPatternWithRecordOfFunction, compiler/indexedAccessRetainsIndexSignature, compiler/localTypeParameterInferencePriority, compiler/omitTypeTests01, compiler/recursiveGenericUnionType1, compiler/recursiveGenericUnionType2, compiler/specialIntersectionsInMappedTypes, compiler/varianceProblingAndZeroOrderIndexSignatureRelationsAlign, compiler/varianceProblingAndZeroOrderIndexSignatureRelationsAlign2, conformance/mappedTypeIndexSignatureModifiers, conformance/namedTupleMembers, conformance/staticIndexSignature5
- **Also blocked (other clusters needed) (16):** compiler/aliasOfGenericFunctionWithRestBehavedSameAsUnaliased (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/contextualTypeBasedOnIntersectionWithAnyInTheMix5 (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, unaligned); compiler/correlatedUnions (+FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS, LITERAL-WIDENING, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, UNATTRIBUTED, unaligned); compiler/declarationEmitMappedTypeDistributivityPreservesConstraints [type-operators] (+TYPE-ALIAS-ACCESSIBILITY-GATE); compiler/deeplyNestedConstraints (+CONDITIONAL-DISTRIBUTIVE-CONSTRAINT); compiler/deeplyNestedMappedTypes (+CONDITIONAL-TYPE-ROOT-ALIAS, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/discriminantPropertyCheck (+UNATTRIBUTED); compiler/genericRestTypes (+FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS, NODEBUILDER-GET-REDUCED-TYPE); compiler/indexedAccessAndNullableNarrowing (+NARROWABLE-REF-INFERENTIAL-GATE, unaligned); compiler/indexingTypesWithNever (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, unaligned); compiler/renamingDestructuredPropertyInFunctionType3 (+CLONE-BINDING-NAME); compiler/symbolLinkDeclarationEmitModuleNames (+SPECIFIER-FOR-MODULE-SYMBOL); compiler/symbolLinkDeclarationEmitModuleNamesRootDir (+SPECIFIER-FOR-MODULE-SYMBOL); conformance/intersectionTypeInference3 (+CHOOSE-OVERLOAD-GENERIC-WALK); conformance/keyofIntersection (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); conformance/mappedTypes4 (+COMPARE-TYPES-ARMS, FORIN-VARIABLE-INDEX-TYPE)

### 5. `QUALIFIED-NAME-LEFT-ALIAS-RESOLVE` (V; blocked 18, finished alone 12, lines 90; confidence high)

Merged from wave-2 clusters: `QUALIFIED-NAME-ALIAS-LEFT-RESOLVE`, `QUALIFIED-ENTITY-LEFT-ALIAS` (G05-QUALIFIED-TYPEREF, G07-ALIAS-RESOLVE). They share the same upstream operation and TSR site.

- **Root cause:** resolveQualifiedName resolves the LEFT of `X.Y` with resolveEntityName(left, Namespace), whose alias loop (resolveAlias) follows import-equals / `import * as` / ES-import / default-import aliases to the namespace or module; TSR's resolve_entity_name reads `exports` straight off the unresolved ALIAS symbol (always empty), so every alias-rooted qualified type reference misses and falls to unresolved_type_reference: it prints the written text but is_error, so members, unions, intersections and keyof over it answer error. / resolveQualifiedName resolves the left side with resolveEntityName(Namespace, dontResolveAlias=false), so an alias left is resolveAlias'd before its exports are read; TSR reads exports straight off the alias symbol, which has none.
- **tsgo:** internal/checker/checker.go:15828 resolveQualifiedName (left via resolveEntityName at :15829; alias loop :15821; re-export fallback :15853) ; internal/checker/checker.go:15828 resolveQualifiedName (+15821 resolveEntityName alias loop)
- **TSR:** crates/tsr-checker/src/declared.rs:4841 resolve_entity_name (QualifiedName arm; miss lands in qualified_type_reference at declared.rs:4695 -> unresolved_type_reference :4706) ; crates/tsr-checker/src/symbols.rs:1262 resolve_qualified_entity (also crates/tsr-conformance/src/types_producer.rs:1301 entity_name_symbol leaf rule)
- **Examples:**
  - `compiler/internalAliasUninitializedModule`: `x.foo`. want `() => any`, got `error`. `import b = a.b; var x: b.I; x.foo()`: tsgo resolves left `b` through the alias to namespace a.b and finds interface I. TSR's resolve_entity_name looks up `I` in the alias symbol's own (empty) exports, mints an unresolved `b.I` (is_error), so x.foo is error; probe q2.ts (import b = A.inA; p.y) oracle: tsgo number, TSR error.
  - `compiler/reactTagNameComponentWithPropsNoOOM`: `Tag`. want `keyof React.ReactHTML`, got `error`. `import * as React; declare const Tag: keyof React.ReactHTML`: the namespace-import alias is the left of the qualified name, so TSR's lookup misses and keyof over the unresolved (error) mint is error. With react16 inlined, the oracle (r6.tsx) prints `Tag : keyof React.ReactHTML`, `el : React.ReactElement<any> \| null`, and members of `React.ComponentClass` work. TSR gives error for all three.
  - `conformance/importStatements`: `x.x`. want `number`, got `error`. `import a = A; var x: a.Point` while `A.Point` directly works: probe q2/q3 show the same split for import= and `import * as M` (tsgo `x.x : number`, `u : number \| M.I`; TSR error both).
- **Port plan:** In resolve_entity_name's QualifiedName arm, port resolveQualifiedName: resolve the left with meaning NAMESPACE, then run resolveEntityName's alias loop (while flags&meaning==0 && ALIAS: symbol = resolve_alias(symbol)), and look the right name up in getExportsOfSymbol(namespace). For a module or export= target this means resolveExternalModuleSymbol's exports, not raw `exports`. Keep the :15853 fallback (exports of resolveAlias(namespace) when the first lookup misses on an alias). Also apply the same alias loop when the final symbol is an alias (resolveEntityName :15821). Do this before or together with QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE: the resolved symbol then takes the existing resolved road. Pitfalls: (1) callsOnComplexSignatures 0:264/270 use `import React from "react"` with esModuleInterop, so resolve_alias needs the synthetic default (import_clause_default_target only ports the plain half, see the SYNTHETIC-DEFAULT family). (2) Keep §605's circular-alias gate (declared.rs:4696) for circular4. (3) Printing: tsgo spells via the accessible symbol chain (`keyof M.I` even when `N.I` was written, probe r5). TSR's written-text mint keeps the written spelling, so the NB-SYMBOL-CHAIN family owns any residual spelling differences.

In resolve_qualified_entity, after resolving the root (NAMESPACE) and after each intermediate segment, loop `while flags & NAMESPACE == 0 && flags & ALIAS != 0 { symbol = resolve_alias(symbol) }`, as resolveEntityName does at 15821. Do not run the loop on the final segment (getSymbolOfPartOfRightHandSideOfImportEquals passes dontResolveAlias=true). Apply the same alias resolution to types_producer.rs entity_name_symbol's container at 1301, and to declared.rs:1489's fallback that shares this function. Also handle the root-identifier line (types_producer:1244), which currently prints any for an alias root such as `basics`.
- **Finished alone (12):** compiler/contextuallyTypedJsxChildren2, compiler/declarationEmitRetainedAnnotationRetainsImportInOutput, compiler/externalModuleReferenceDoubleUnderscore1, compiler/internalAliasUninitializedModule, compiler/internalAliasUninitializedModuleInsideLocalModuleWithExport, compiler/internalAliasUninitializedModuleInsideLocalModuleWithoutExport, compiler/internalAliasUninitializedModuleInsideLocalModuleWithoutExportAccessError, compiler/internalAliasUninitializedModuleInsideTopLevelModuleWithoutExport, compiler/reactTagNameComponentWithPropsNoOOM, compiler/reactTagNameComponentWithPropsNoOOM2, compiler/trackedSymbolsNoCrash, conformance/importStatementsInterfaces
- **Also blocked (other clusters needed) (6):** compiler/callsOnComplexSignatures (+PRINT-ALIAS-DEFAULT-TYPEARGS, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, SYNTHETIC-DEFAULT-IMPORT-TARGET); compiler/declarationEmitEnumReferenceViaImportEquals (+DECLARED-TYPE-OF-ALIAS); compiler/jsxComplexSignatureHasApplicabilityError (+CLASS-GET-BASE-TYPES, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, unaligned); compiler/jsxNamespaceGlobalReexportMissingAliasTarget (+QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE); conformance/importStatements (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); conformance/umd5 (+TRYSYMBOLTABLE-UMD-ALIAS-EXCLUSION)

### 6. `SYNTHETIC-DEFAULT-IMPORT-TARGET` (V; blocked 18, finished alone 12, lines 58; confidence high)

- **Root cause:** getTargetOfModuleDefault's `hasSyntheticDefault || hasDefaultOnly` arm (resolve the default import to resolveExternalModuleSymbol(module), i.e. the export= target or the module itself) is ported only for narrow slices: TSR's module_default_target requires module=CommonJS + non-declaration-file chains, its ambient fallback rejects NAMESPACE/CLASS/FUNCTION targets, it never returns the module itself for a no-default/no-export= declaration file, and can_have_synthetic_default lacks the node16/nodenext ESM->CJS arm.
- **tsgo:** internal/checker/checker.go:14536 getTargetOfModuleDefault (arm :14578-14585), checker.go:14818 canHaveSyntheticDefault (node arm :14823-14848), checker.go:14800 isOnlyImportableAsDefault
- **TSR:** crates/tsr-checker/src/symbols.rs:1386 module_default_target (with crates/tsr-checker/src/check.rs:2660 can_have_synthetic_default)
- **Examples:**
  - `compiler/esModuleInteropDefaultMemberMustBeSyntacticallyDefaultExport`: `Point`. want `typeof Point`, got `any`. index.ts `import Point from "./point"` where point.d.ts is `export = Point` (class); canHaveSyntheticDefault's declaration-file arm is true (static `default` is not syntactic), so tsgo resolves to the class. TSR's synthetic arm rejects declaration files and the ambient fallback rejects non-ambient/class targets -> None -> any (C/p/new C downstream error).
  - `compiler/allowSyntheticDefaultImports1`: `Namespace`. want `typeof Namespace`, got `any`. b.d.ts has no default and no export=; canHaveSyntheticDefault (decl-file arm) is true so resolveExternalModuleSymbol returns the module itself, printed via the local alias. TSR's clause path returns None when resolved==module.
  - `compiler/tsxSpreadDoesNotReportExcessProps`: `React`. want `typeof React`, got `any`. `import React from "react"` against react16.d.ts ambient `declare module "react" { export = React }`; ambient -> file==nil -> synthetic default true -> React namespace. TSR's ambient fallback (symbols.rs:1529) explicitly declines NAMESPACE targets; all React-lib cases (callsOnComplexSignatures, contextuallyTyped*, jsx*, tsx*, controlFlowOptionalChain3, unionReduction...) are this one witness.
- **Port plan:** Replace module_default_target's three ad-hoc arms with a faithful getTargetOfModuleDefault: compute exportDefault via resolveExportByName(module,'default'), hasDefaultOnly (isOnlyImportableAsDefault: nodenext ESM usage of a JSON target) and hasSyntheticDefault = full canHaveSyntheticDefault taking the import's specifier as usage; if either is true return resolve_external_module_symbol(module) (the module itself when there is no export=), else the real default. Extend check.rs can_have_synthetic_default with the node16..nodenext block (:14823-14848): needs per-file GetImpliedNodeFormatForEmit and usage-site emit syntax, which ModuleHost (resolution.rs:92) does not expose today though tsr-compiler/src/loader.rs:1241 implied_node_format_for_emit computes it — add a host method. Also make the declaration-file arm use isSyntacticDefault on the resolved default (export specifiers count, so `export {x as default}` blocks it - nullthrows case relies on the node arm). PITFALLS: (1) the declines exist for naming (bd tsr-4jk): once a default alias resolves to a module/namespace, tsgo prints every alias of that module by the FIRST accessible alias (oracle /tmp/triage/v_G12/p1.ts: `import * as M from './b'` then prints `typeof N`, `Rn : typeof Rd`), so module_alias_at/type_to_string_at must pick upstream's alias or currently-RIGHT `typeof M` lines flip to WRONG; (2) the comment at symbols.rs:1509-1513 records importEquals1's 6 G->W when a file module gained a default alias — re-measure; (3) delete the specifier-only legacy branch at :1465-1471 only after export-specifier defaults are routed through the same function.
- **Finished alone (12):** compiler/allowSyntheticDefaultImports1, compiler/allowSyntheticDefaultImports4, compiler/esModuleInterop, compiler/esModuleInteropDefaultMemberMustBeSyntacticallyDefaultExport, compiler/esModuleInteropEnablesSyntheticDefaultImports, compiler/exportAssignmentWithoutAllowSyntheticDefaultImportsError, compiler/jsxSpreadFirstUnionNoErrors, compiler/nodeNextEsmImportsOfPackagesWithExtensionlessMains, compiler/nodeNextImportModeImplicitIndexResolution2, compiler/tsxReactPropsInferenceSucceedsOnIntersections, compiler/tsxSpreadDoesNotReportExcessProps, conformance/controlFlowOptionalChain3
- **Also blocked (other clusters needed) (6):** compiler/callsOnComplexSignatures (+PRINT-ALIAS-DEFAULT-TYPEARGS, QUALIFIED-NAME-LEFT-ALIAS-RESOLVE, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE); compiler/contextuallyTypedJsxAttribute2 (+EXTERNAL-MODULE-MEMBER-EXPORT-EQUALS); compiler/contextuallyTypedJsxChildren (+QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE); compiler/jsxPartialSpread (+SIGNATURE-DECLARATION-ANNOTATION-REUSE); compiler/nodeNextCjsNamespaceImportDefault2 (+RESOLVE-ES-MODULE-SYMBOL-CLONE); compiler/unionReductionWithStringMappingAndIdenticalBaseTypeExistsNoCrash (+, unaligned)

### 7. `NULL-WIDENING-TYPES-MISSING` (V; blocked 13, finished alone 12, lines 83; confidence high)

- **Root cause:** With strictNullChecks off, null/undefined expressions are nullWideningType/undefinedWideningType (ContainsWideningType) and addTypesToUnion drops nullables so an all-nullable union returns the widening null; TSR has no null widening intrinsic, so check_object_literal_members refuses any nullable member (error) and union_type_worker's empty-set arm answers error.
- **tsgo:** internal/checker/checker.go:25027 createWideningType (+ :990 nullWideningType, :25653 getUnionTypeWorker empty-typeSet arm, :13144 checkObjectLiteral, :18400 getWidenedTypeOfObjectLiteral)
- **TSR:** crates/tsr-checker/src/objects.rs:867 check_object_literal_members (refusal at :1734); crates/tsr-checker/src/unions.rs:570 union_type_worker (gap at :697); crates/tsr-checker/src/intrinsics.rs:58 (only undefined has a loose widening twin)
- **Examples:**
  - `conformance/objectLiteralWidened`: `{    foo: null,    bar: undefined}`. want `{ foo: null; bar: undefined; }`, got `error`. @strict false literal; tsgo member types are null/undefinedWidening, literal prints raw and the decl widens to {foo:any;bar:any}. TSR objects.rs:1734 returns error for any NULLABLE member when !strict_null_checks (probed `{y:null}` -> error).
  - `compiler/conditionalExpressions2`: `false ? null : undefined`. want `null`, got `error`. Non-strict union of null and undefined empties the type set; getUnionTypeWorker returns nullWideningType. TSR unions.rs:697 deliberately returns error.
  - `conformance/decrementOperatorWithAnyOtherType`: `obj`. want `{ x: number; y: any; }`, got `error`. `var obj = {x:1,y:null}` literal gaps at objects.rs:1734, so the declaration and every obj.x/obj.y use gap downstream.
- **Port plan:** Add null_widening and undefined_widening intrinsics (createWideningType: same flags, ObjectFlags ContainsWideningType, distinct ids) and make non-strict `null`/`undefined` expressions answer them (the existing loose_undefined_widening seam in intrinsics.rs:188 covers undefined only). In add_types_to_union record IncludesNonWideningType for nullables without ContainsWideningType and drop all nullables from the set when !strict; port the empty-set arm (null beats undefined, non-widening vs widening). Delete the objects.rs:1734 refusal and propagate ContainsWideningType through object-literal member flags so getWidenedTypeOfObjectLiteral (already in widening.rs widen_object_in_context) maps nullable props to any. Pitfall: printing must stay `null`/`undefined` for the widening twins and identity comparisons (`== intrinsics.null`) across the checker need to accept both.
- **Finished alone (12):** compiler/conditionalExpressions2, compiler/declFileRegressionTests, compiler/null, compiler/overloadResolutionOverNonCTObjectLit, compiler/typeParameterFixingWithConstraints, conformance/arrayLiterals2ES5, conformance/computedPropertyNames5_ES6, conformance/decrementOperatorWithAnyOtherType, conformance/incrementOperatorWithAnyOtherType, conformance/objectLiteralWidened, conformance/propertyNameWithoutTypeAnnotation, conformance/symbolProperty19
- **Also blocked (other clusters needed) (1):** compiler/widenedTypes1 (+WIDEN-NULLABLE-TO-ANY)

### 8. `MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT` (V; blocked 28, finished alone 10, lines 135; confidence medium/high)

Merged from wave-2 clusters: `CONDITIONAL-NODE-WRITTEN-MINT-QUALIFIED`, `MAPPED-CONDITIONAL-WRITTEN-TEXT`, `MAPPED-TYPE-TEXT-MINT-PRINT`, `MAPPED-INLINE-NODE-INSTANTIATION`, `MAPPED-NODE-MINT-AS-CLAUSE`, `CONDITIONAL-TYPE-NODE-MINT`, `CONDITIONAL-TYPE-TO-TYPE-NODE` (G05-QUALIFIED-TYPEREF, G13-NONSTRICT-NULL, G14-MAPPED-CONDITIONAL, G14-MAPPED-CONDITIONAL, G14-MAPPED-CONDITIONAL, G14-MAPPED-CONDITIONAL, G16-NB-NAMING). They share the same upstream operation and TSR site.

- **Root cause:** A deferred conditional type is a ConditionalType object upstream, printed by the node builder from its instantiated parts (defaults filled, qualified names via symbol chain). TSR mints it from written_type_text, which declines (-> error) whenever the node contains a qualified type reference such as `Tools.Cast<G, any[]>`. / tsgo resolves mapped/conditional type nodes to MappedType/ConditionalType and prints them via typeToTypeNode, so unions inside print in CompareTypes order; TSR represents them as TypeData::Named holding the WRITTEN text (declared.rs written_type_text), preserving source union order. / TSR prints every mapped type from a text string minted from the written node (mapped_type_text), whereas tsgo's nodebuilder prints non-generic mapped types as their resolved members and generic ones from semantic parts (getTemplateTypeFromMappedType with addOptionality, filled default type args). / A mapped type node written inline inside a conditional branch (not via an alias) is minted once from the written node and never instantiated with the conditional's mapper (instantiateMappedType); via an alias it works. / getTypeFromMappedTypeNode always creates a MappedType; TSR's mapped mint declines (error) for an as-clause with a template-literal name type plus a function-type template, and errors instantiating template-literal as-clauses. / getTypeFromConditionalTypeNode always creates a ConditionalRoot type; TSR's written-text mint declines (error) when the check type is `typeof x` or when `infer` appears inside a function/constructor type in the extends type. / conditionalTypeToTypeNode prints a deferred conditional from the type (check/extends via typeToTypeNode with parenthesisation, branches via getTrueType/getFalseTypeFromConditionalType); TSR prints deferred conditionals from written annotation text, which declines function-type/typeof/array operands (-> error), keeps source union order, and embeds local alias names.
- **tsgo:** internal/checker/checker.go:24269 getTypeFromConditionalTypeNode ; internal/checker/checker.go getTypeFromMappedTypeNode / getTypeFromConditionalTypeNode, printed by nodebuilderimpl.go typeToTypeNode (union members from the sorted type) ; internal/checker/nodebuilderimpl.go:2690 createTypeNodeFromObjectType (isGenericMappedType gate) + internal/checker/checker.go:22697 getTemplateTypeFromMappedType ; internal/checker/checker.go:24255 getTypeFromMappedTypeNode + :22535 instantiateMappedType (reached from getConditionalType :24427 instantiateType(trueType, trueMapper)) ; internal/checker/checker.go:24255 getTypeFromMappedTypeNode ; internal/checker/nodebuilderimpl.go:2916 conditionalTypeToTypeNode
- **TSR:** crates/tsr-checker/src/declared.rs:447 get_type_from_type_node MappedTypeNode|ConditionalTypeNode arm (written_type_text at :494; signatures.rs:5264 written_type_text) ; crates/tsr-checker/src/declared.rs:447 get_type_from_type_node MappedTypeNode|ConditionalTypeNode arm (written_type_text at :494); mapped.rs:56 mapped_type_text ; crates/tsr-checker/src/mapped.rs:56 mapped_type_text (called from create_semantic_mapped_type mapped.rs:45 and instantiate_mapped_type_worker mapped.rs:838) ; crates/tsr-checker/src/declared.rs:447 get_type_from_type_node MappedTypeNode|ConditionalTypeNode arm ; crates/tsr-checker/src/mapped.rs:45 create_semantic_mapped_type (mapped_type_info :255 / mapped_type_text :56 return None) ; crates/tsr-checker/src/signatures.rs:5301 written_type_text_flags (ConditionalTypeNode arm) + declared.rs:228 get_type_from_type_node conditional arm
- **Examples:**
  - `compiler/ramdaToolsNoInfinite`: `args`. want `GapsOf<T, Parameters<F>, [], []> extends infer G ? Tools.Cast<G, any[]> : never`, got `error`. `(...args: GapsOf<T, Parameters<F>> extends infer G ? Tools.Cast<G, any[]> : never)`: tsgo builds a deferred ConditionalType and prints it, filling GapsOf's defaults. Probe c1.ts: the same conditional with a same-file `Tools.Cast` gives TSR `x : error`, while the local alias `Cast2` prints the conditional text.
  - `compiler/declarationEmitMappedTypePropertyFromNumericStringKey`: `arg`. want `{ [K in keyof T]: string \| T[K]; }`, got `{ [K in keyof T]: T[K] \| string; }`. Template `T[K] \| string` sorted: String 1<<5 < IndexedAccess 1<<25. TSR keeps written text (oracle on ord.ts, same for `typeof a`).
  - `compiler/complicatedIndexesOfIntersectionsAreInferencable`: `x`. want `string extends "initialValues" \| "validate" \| keyof ExtraProps ? ...`, got `string extends "validate" \| "initialValues" \| keyof ExtraProps ? ...`. Conditional node printed from written text; the bare union `"validate"\|"initialValues"` sorts correctly in TSR (probe), so only the written-text route diverges.
- **Port plan:** This is not a type-reference resolution bug. Either teach written_type_text to spell qualified references (and default-filled arguments, as `Tools.Drop<..., []>` and `GapsOf<..., [], []>` require), or, faithfully, build a real ConditionalType (getTypeFromConditionalTypeNode: root with checkType/extendsType/inferTypeParameters, deferred while generic) and print it structurally. The faithful port supersedes the print-only mint at declared.rs:447-575. Likely merges with the CONDITIONAL-RESOLVE-OR-DEFER family that also blocks this case.

Where the written-text arm is taken, print the template/check/extends parts via type_to_string of the resolved component types (as mapped_type_text already does for the semantic fallback) instead of the raw node text — or route all mapped types through create_semantic_mapped_type. Keep written text only where tsgo reuses nodes (declaration signatures: tsgo still prints `arg: { [K in keyof T]: T[K] | string; }` in the parameter position). Pitfall: dependentDestructuredVariablesFromNestedPatterns also has MAPPED-TYPE/other families.

Stop baking a text name into mapped types: make mapped types print through a ported createTypeNodeFromObjectType: if isGenericMappedType (or containsError) emit `{ [P in C as N]?: Template }` from semantic constraint/name/template where template = getTemplateTypeFromMappedType (addOptionality under `?` with strictNullChecks; type references printed with filled defaults), else print resolved members (resolve_mapped_type_members). Delete mapped_type_text and the two store.new_named(text) mints plus the render_object_type re-mint in instantiate_mapped_type_worker. Pitfall: declaration-signature lines reuse the written node (NB-TYPENODE-REUSE) so the signature print still shows the bare form; keep member order from the constraint union order (union printing order differs from written order, e.g. `{ x: string; } | null`). correlatedUnions `...args` and mappedTypeGenericIndexedAccess `!this.entries[name]` GAP lines are downstream of the same type and may also need indexed-access on the resolved type.

Have the MappedTypeNode arm produce a real semantic MappedType (getTypeFromMappedTypeNode: declaration + alias + outer type parameters) and let the conditional branch path instantiate it with instantiate_mapped_type under the current mapper (alias_evaluation_bindings) instead of returning the written mint. Shares the mint removal with MAPPED-TYPE-TEXT-MINT-PRINT; do that first.

Replace the declining mint with an unconditional semantic MappedType (part of the MAPPED-TYPE-TEXT-MINT-PRINT cutover), then port as-clause member resolution (resolveMappedTypeMembers forEachType(getNameTypeFromMappedType instantiated per key)) so template-literal names instantiate.

Replace the print-only conditional mint with a ConditionalRoot (checkType, extendsType, infer type parameters from the node's locals, outer type parameters, isDistributive) built by getTypeFromConditionalTypeNode, evaluated via getConditionalType with nil mapper; printing a deferred conditional from its parts (infer params renamed by typeParameterToName). Do with CONDITIONAL-INLINE-NODE-INSTANTIATION since both need a semantic deferred conditional. inferTypes2's 0:0/0:2/0:4/0:5 lines belong here (case also listed under inline instantiation for 0:11/0:13).

Give deferred conditional types a real ConditionalType representation (root check/extends/true/false + mapper) and port conditionalTypeToTypeNode into the printer (parenthesise function/constructor types in extends, conditional in union/array). Remove the ConditionalTypeNode arm from written_type_text_flags and the 'deferred one keeps the written mint' path. Pitfall: contextualOuterTypeParameters/controlFlowInstanceof also blocked by CONTEXTUAL-SIG-TYPEPARAMS / NARROW-INSTANCEOF-HASINSTANCE.
- **Finished alone (10):** compiler/conditionalTypeBasedContextualTypeReturnTypeWidening, compiler/declarationAssertionNodeNotReusedWhenTypeNotEquivalent1 [type-operators], compiler/declarationEmitMappedTypePropertyFromNumericStringKey [type-operators], compiler/declarationQuotedMembers [type-operators], compiler/mappedArrayTupleIntersections [type-operators], compiler/mappedTypeMultiInference [type-operators], compiler/mappedTypeWithAsClauseAndLateBoundProperty2, compiler/simplifyingConditionalWithInteriorConditionalIsRelated [type-operators], compiler/wideningWithTopLevelTypeParameter [type-operators], conformance/assignmentGenericLookupTypeNarrowing [type-operators]
- **Also blocked (other clusters needed) (18):** compiler/complicatedIndexesOfIntersectionsAreInferencable (+CONTEXTUAL-DEFERRED-CONDITIONAL, SIGNATURE-DECLARATION-ANNOTATION-REUSE); compiler/contextualOuterTypeParameters (+CONTEXTUAL-SIGNATURE-TYPEPARAM-ADOPTION); compiler/controlFlowInstanceofWithSymbolHasInstance (+NARROW-INSTANCEOF-HASINSTANCE); compiler/correlatedUnions (+FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS, LITERAL-WIDENING, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, UNATTRIBUTED, unaligned); compiler/declarationEmitNestedGenerics (+SHADOWED-TYPEPARAM-RENAME); compiler/declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1 (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/declarationEmitOptionalMappedTypePropertyNoStrictNullChecks2 (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/declarationEmitOptionalMappedTypePropertyNoStrictNullChecks3 (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/declarationEmitShadowingInferNotRenamed (+, unaligned); compiler/deeplyNestedMappedTypes (+CONDITIONAL-TYPE-ROOT-ALIAS, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/mappedTypeGenericIndexedAccess [type-operators] (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/mappedTypeTupleConstraintAssignability [type-operators] (+, unaligned); compiler/ramdaToolsNoInfinite (+CONDITIONAL-DEFERRAL-GATE, INSTANTIATE-MAPPED-TYPE-ALIAS-DROP); compiler/verbatim-declarations-parameters [type-operators] (+PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED); conformance/dependentDestructuredVariablesFromNestedPatterns (+APPARENT-MAPPED-ARRAY-CONSTRAINT, DESTRUCTURE-FLOW-DEPENDENT, MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS, UNATTRIBUTED); conformance/inferTypes2 (+CONDITIONAL-INLINE-NODE-INSTANTIATION); conformance/isomorphicMappedTypeInference (+FORIN-VARIABLE-INDEX-TYPE, INFERENCE-REVERSE-MAPPED-MEMBER, OBJLIT-PROPERTY-SYMBOL-TYPE); conformance/mappedTypeModifiers [type-operators] (+MAPPED-TEMPLATE-ADD-OPTIONALITY-INDEX)

### 9. `REORDER-CANDIDATES` (V; blocked 12, finished alone 10, lines 90; confidence high)

- **Root cause:** resolveCall's reorderCandidates (hoist literal-typed 'specialized' signatures; splice later declaration groups of a merged symbol ahead of earlier ones) is ported only for the construct road; the call road keeps declaration order and its subtype pass refuses (Undecidable) any set containing a specialized signature.
- **tsgo:** internal/checker/checker.go:8957 reorderCandidates (called from :8843 resolveCall)
- **TSR:** crates/tsr-checker/src/calls.rs:2415 subtype_pass_outcome (literal guard L2432) + calls.rs:1720 choose_overload / calls.rs:1416 resolve_call_signature_at; existing port signatures.rs:758 reorder_construct_candidates (only used at expressions.rs:2520)
- **Examples:**
  - `conformance/twoMergedInterfacesWithDifferingOverloads`: `b.foo(true)`. want `Date`, got `number`. B<T> declared twice; later group foo(x:T):Date is spliced at index 0 by reorderCandidates (different parent -> index=cutoffIndex). TSR walks declaration order and picks foo(x:T):number; probe `interface A{foo(x:number):number} interface A{foo(x:number):string}` a.foo(1): tsgo string, TSR number (oracle).
  - `conformance/interfaceWithSpecializedCallAndConstructSignatures`: `f('a')`. want `number`, got `any`. (x:'a'):number has SignatureFlagsHasLiteralTypes and is hoisted; first applicable wins. TSR subtype_pass_outcome returns Undecidable on any specialized candidate; oracle probe g('a') tsgo number / TSR error.
  - `conformance/stringLiteralTypesOverloads01`: `getFalsyPrimitive("string")`. want `string`, got `error`. Literal-param overloads; same Undecidable guard then ambiguous_return guard (calls.rs:2040) -> error.
- **Port plan:** Generalize signatures.rs reorder_construct_candidates into a reorderCandidates port (symbol/parent grouping, cutoffIndex, specializedIndex splice, callChainFlags optional-call clone) and apply it once in the call road before choose_overload (resolve_call_signature_at and tagged-template road), same as resolveCall. Then delete the signature_has_literal_types Undecidable guard in subtype_pass_outcome and the 'declaration order' comments. Pitfalls: signatureHasLiteralTypes is a syntactic flag (enum member refs not specialized); merged groups across files must keep upstream symbol/parent identity (late-bound members in symbolProperty41 also need LATE-BOUND-NAMES); the ambiguous_return guard at calls.rs:2040 will still block if left in place for reordered sets.
- **Finished alone (10):** compiler/inheritedOverloadedSpecializedSignatures, compiler/overloadBindingAcrossDeclarationBoundaries, conformance/interfaceWithSpecializedCallAndConstructSignatures, conformance/numericLiteralTypes1, conformance/numericLiteralTypes2, conformance/stringLiteralTypesOverloads01, conformance/stringLiteralTypesOverloads02, conformance/stringLiteralTypesOverloads03, conformance/twoMergedInterfacesWithDifferingOverloads, conformance/typesWithSpecializedCallSignatures
- **Also blocked (other clusters needed) (2):** compiler/overloadBindingAcrossDeclarationBoundaries2 (+DECL-NAME-MERGED-SYMBOL); conformance/symbolProperty41 (+LATE-BOUND-SIGNATURES-OF-SYMBOL-IMPL-SKIP)

### 10. `ASYNC-YIELD-STAR-ITERATION-TYPES` (V; blocked 10, finished alone 9, lines 68; confidence high)

- **Root cause:** getIterationTypesOfIterable with IterationUseAsyncYieldStar (async iterable first, then sync iterable via getAsyncFromSyncIterationTypes) is unported, so every yield* inside an async generator answers error/next-slot for the expression and declines the containing AsyncGenerator inference for any non-array delegate.
- **tsgo:** internal/checker/checker.go:10952 checkYieldExpression (use = AsyncYieldStar) -> :6287 getIterationTypesOfIterableWorker (async arm) -> :6436 getAsyncFromSyncIterationTypes; aggregation :20322 checkAndAggregateYieldOperandTypes
- **TSR:** crates/tsr-checker/src/expressions.rs:3456 check_yield_expression (is_async gates 3533-3545, 3576-3589); crates/tsr-checker/src/signatures.rs:2497 return_type_from_body yield* arm 2600-2667; existing partial: crates/tsr-checker/src/symbols.rs:4567 for_await_of_yield_types
- **Examples:**
  - `conformance/emitter.asyncGenerators.functionDeclarations.es2018`: `x  (const x = yield* (async function*() { yield 1; })(); in async function* f5)`. want `void`, got `error`. Async fast path on AsyncGenerator<number, void, unknown> gives return void and yield number, so x: void and f5: () => AsyncGenerator<number, void, unknown>. TSR's yield* aggregation only accepts arrays, and the expression's family arm never fires (arity-1 global lookup), so x, the yield* and f5 are error.
  - `conformance/types.asyncGenerators.es2018.1`: `yield* [1, 2]  (async function* inferReturnType6)`. want `any`, got `error`. AsyncYieldStar over number[]: no [Symbol.asyncIterator], so the sync ArrayIterator types are lifted by getAsyncFromSyncIterationTypes; under @strict:false TReturn = BuiltinIteratorReturn = any. TSR gates its array arm on !is_async and falls through to error (probe a5 @strict:true: tsgo undefined, TSR error).
  - `compiler/asyncYieldStarContextualType`: `yield* await authorPromise.then(mapper)`. want `Author`, got `unknown`. Result<Author, E> only has a sync [Symbol.iterator](): Generator<E, Author, unknown>; async-from-sync gives return Author. TSR's annotated branch returns the container annotation's TNext (unknown).
- **Port plan:** Prerequisite: the sync engine of YIELD-STAR-ITERATION-TYPES-OF-ITERABLE. Add the async resolver: getIterationTypesOfIterableFast with the async globals (AsyncIterable/AsyncIteratorObject/AsyncIterableIterator/AsyncGenerator), slow path via [Symbol.asyncIterator]() with awaited next/return/throw results, and for AllowsSyncIterables uses (AsyncYieldStar, ForAwaitOf) fall back to the sync engine wrapped by getAsyncFromSyncIterationTypes (await yield and return; next unchanged). Wire use = isAsync ? AsyncYieldStar : YieldStar in check_yield_expression and in the aggregation, where the yield slot is getAwaitedType(iterated type) per getYieldedTypeOfYieldExpression. Fold for_await_of_yield_types into this engine instead of keeping a second yield-only copy. Pitfall: the comment in for_await_of_yield_types says pinned getAsyncFromSyncIterationTypes panics on a nil yield; mirror the exact nil handling rather than inventing a recovery.
- **Finished alone (9):** compiler/asyncYieldStarContextualType, conformance/emitter.asyncGenerators.classMethods.es2015, conformance/emitter.asyncGenerators.classMethods.es2018, conformance/emitter.asyncGenerators.functionDeclarations.es2015, conformance/emitter.asyncGenerators.functionDeclarations.es2018, conformance/emitter.asyncGenerators.functionExpressions.es2015, conformance/emitter.asyncGenerators.functionExpressions.es2018, conformance/emitter.asyncGenerators.objectLiteralMethods.es2015, conformance/emitter.asyncGenerators.objectLiteralMethods.es2018
- **Also blocked (other clusters needed) (1):** conformance/types.asyncGenerators.es2018.1 (+CONTEXTUAL-RETURN-IIFE-ARM, GENERATOR-ANNOTATION-ITERATION-TYPES)

### 11. `GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE` (V; blocked 13, finished alone 8, lines 36; confidence high)

- **Root cause:** getTypeOfNode only evaluates an identifier as a value when ast.IsExpressionNode is true and otherwise falls through to errorType; TSR's type_id_at_location_tracking ends with an ungated check_expression (plus a label arm that keeps a resolved value), so non-expression identifiers (labels, require(x) argument, JSX namespaced-name parts, import-type qualifiers, inner qualified-name segments) are resolved as free names.
- **tsgo:** internal/checker/checker.go:31955 getTypeOfNode (IsExpressionNode gate; fallthrough return c.errorType at :32035)
- **TSR:** crates/tsr-conformance/src/types_producer.rs:1577 type_id_at_location_tracking (final check_expression fallthrough; also label arm :1404 and qualified-name-left gate :1214)
- **Examples:**
  - `conformance/typeofOperatorWithBooleanType`: `z`. want `any`, got `boolean`. `declare var z: boolean; z: typeof BOOLEAN;` — the label z is not an expression node so getTypeOfNode returns errorType and the writer's IsLabelName guard prints any. TSR's label arm (:1404) check_expressions the label, resolves the var, and keeps boolean (oracle probe p1.ts reproduced: tsgo any, TSR boolean).
  - `compiler/usingModuleWithExportImportInValuePosition`: `a`. want `any`, got `string`. `var c: C.a.B.Id` — `a` is the right of an inner QualifiedName that is the left of an outer one: not part of a type node, not an expression, so errorType (printed any by the QualifiedName parent guard). TSR's left arm skips it because of the `qualified.right != id` gate at :1214, and the final fallthrough resolves the global `var a: string`.
  - `compiler/typeofImportInstantiationExpression`: `myFunction`. want `error`, got `any`. `typeof import('./input.js').myFunction` — the import-type qualifier is not an expression node, so tsgo answers errorType and, with no .errors.txt baseline, prints `error`. TSR's ungated check_expression on the qualifier identifier gives any (reproduced with probefile on the copied fixture).
- **Port plan:** Put upstream's IsExpressionNode gate (tsr_ast::predicates::is_expression_node, already used by `selects`) on the final fallthrough at types_producer.rs:1577. When the node is not an expression and no earlier getTypeOfNode-equivalent arm matched, return error. The writer-side spelling rules at render_case :1850 (had_error_baseline), :1888 (import/export names) and :1979 (PropertyAccess/QualifiedName parent) then turn that error into any, and a label guard (IsLabelName, type_symbol_baseline.go:384) has to be added beside them. Delete the label arm at :1404, which keeps a shadowing value type, and invert the unit test a_label_shadowing_a_value_keeps_the_type_we_computed (upstream prints any for every label). For qualified names, a QualifiedName right that is itself the left of another QualifiedName must also reach the errorType answer; either drop the :1214 gate for non-outermost rights or let the new gate handle it, keeping the TypeQuery and ImportEquals exemptions. Pitfall: identifiers that are expressions only through IsInExpressionContext, and declaration/property names handled by earlier arms, must still be checked first, so the gate belongs only at the fallthrough. JSX namespaced-name parts and import-type qualifiers must print `error`: no writer guard may convert them.
- **Finished alone (8):** compiler/sourceMapSample, conformance/TwoInternalModulesWithTheSameNameAndDifferentCommonRoot, conformance/importNonStringLiteral, conformance/typeofOperatorWithAnyOtherType, conformance/typeofOperatorWithBooleanType, conformance/typeofOperatorWithEnumType, conformance/typeofOperatorWithNumberType, conformance/typeofOperatorWithStringType
- **Also blocked (other clusters needed) (5):** compiler/importUsedInGenericImportResolves (+IMPORT-TYPE-NODE-TYPE-MEANING); compiler/jsxNamespacedNameNotComparedToNonMatchingIndexSignature (+MODULE-AUGMENTATION-MERGE); compiler/typeofImportInstantiationExpression (+IMPORT-TYPE-NODE-TYPEOF, LOGICAL-OR-COALESCE-GENERIC-GATE); compiler/usingModuleWithExportImportInValuePosition (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); conformance/exportImportAlias (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 12. `CLONE-BINDING-NAME` (V; blocked 12, finished alone 8, lines 45; confidence high)

Merged from wave-2 clusters: `CLONE-BINDING-NAME`, `SIGNATURE-PARAM-BINDING-NAME-PRINT` (G10-NB-REUSE, G11-BINDING-ELEMENT). They share the same upstream operation and TSR site.

- **Root cause:** parameterToParameterDeclarationName -> cloneBindingName clones the WRITTEN binding pattern for every shape: holes, string-literal/computed property names, and the elements list's trailing comma, dropping only initializers. TSR's render_binding_pattern re-synthesizes text and declines (None -> the whole signature prints `error`) on holes and on non-identifier property names, and it never emits the trailing comma. / The node builder clones a parameter's binding pattern verbatim (cloneBindingName: holes, string/numeric/computed property names), but TSR's render_binding_pattern declines omitted elements and any non-identifier property name, so the whole signature (and the type alias/variable holding it) answers error.
- **tsgo:** internal/checker/nodebuilderimpl.go:1713 cloneBindingName (via :1691 parameterToParameterDeclarationName) ; internal/checker/nodebuilderimpl.go:1691 parameterToParameterDeclarationName (-> :1713 cloneBindingName)
- **TSR:** crates/tsr-checker/src/signatures.rs:4625 render_binding_pattern (called from parameter_of :4678) ; crates/tsr-checker/src/signatures.rs:4625 render_binding_pattern (None for missing name at :4633 and for non-Identifier property_name at :4652)
- **Examples:**
  - `compiler/declarationEmitDestructuring5`: `baz`. want `([, z, ,]: [any, any, any?]) => void`, got `error`. A hole is a BindingElement with name None (parser expression.rs:2595). render_binding_pattern returns None at :4633, which declines parameter_of and the whole signature; probe bn.ts `function baz([, z, , ])` gives `error`.
  - `conformance/contextuallyTypedBindingInitializer`: `f2`. want `({ "show": showRename }: Show) => void`, got `error`. The string-literal property_name hits `Some(_) => return None` at :4652. tsgo clones the node verbatim; probe f2 gives `error`.
  - `conformance/intraExpressionInferencesJsx`: `Component`. want `<T extends Animations>({ animations, style, }: AnimatedViewProps<T>) => JSX.Element`, got `<T extends Animations>({ animations, style }: AnimatedViewProps<T>) => JSX.Element`. cloneBindingName keeps NodeArray.HasTrailingComma. TSR's formatter ignores NodeFlags::HAS_TRAILING_COMMA, which the parser records (expression.rs:2614/2649); probe `f4({ a, b, })` drops the comma.
- **Port plan:** Replace render_binding_pattern with a clone-and-print of the written pattern. Strip initializers, keep holes (empty slots), property names of every kind (identifier, string literal with original quotes, numeric, computed `[expr]` printed from source, e.g. `[(a = "")]`), rest tokens and the HAS_TRAILING_COMMA flag. Reuse tsr-printer's binding-pattern emitter (crates/tsr-printer/src/lib.rs:654 emit with OBJECT/ARRAY_BINDING_PATTERN_ELEMENTS list formats, single-line), or the declarations crate's clone (tsr-declarations type_builder.rs:898), rather than a hand-rolled formatter. Then delete render_binding_pattern's decline arms. Pitfall: printing `[, z, ,]` requires the printer's hole + trailing-comma rule; `[z, ]` must print `[z]`, as baseline a5 shows.

Make render_binding_pattern print what cloneBindingName prints: an omitted element renders as an empty slot (`[_a, , __b]`, `[, ...a]`), and property names render by kind — Identifier as text, StringLiteral quoted as written (`"a"`), NumericLiteral as written (`2`), ComputedPropertyName as `[<expression text>]` (`["a"]`, `[2]`, `[sym]`). Return None only for forms that genuinely have no textual clone. Pitfall: also verify binding_patterns.rs binding_pattern_property_name builds `{ 2: any; }` for numeric/computed-literal names (f9/f11 want that implied type), and that the type-alias lines F10/F11/F13/G10/G11/G13 additionally need BINDING-PATTERN-IMPLIED-TYPE (unannotated params in function types) and F12/G12/F14/G14 need BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS for their `typeof`-free signature only through rendering (their element lines are in that cluster). declarationEmitComputedNameCausesImportToBePainted 1:7/1:8 (family NB-BINDING-NAME, outside this group) is the same renderer arm.
- **Finished alone (8):** compiler/arrayBindingPatternOmittedExpressions, compiler/computedPropertyNameWithImportedKey, compiler/declarationEmitDestructuring5, compiler/unusedParametersWithUnderscore, conformance/arrowFunctionExpressions, conformance/contextuallyTypedBindingInitializer, conformance/controlFlowParameter, conformance/emitArrowFunctionES6
- **Also blocked (other clusters needed) (4):** compiler/declarationEmitComputedNameCausesImportToBePainted (+BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS); compiler/renamingDestructuredPropertyInFunctionType (+BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS, BINDING-PATTERN-IMPLIED-TYPE); compiler/renamingDestructuredPropertyInFunctionType3 (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); conformance/intraExpressionInferencesJsx (+INTRA-EXPRESSION-INFERENCE-JSX-SPREAD)

### 13. `TYPE-ONLY-IMPORT-EXPORT-NAME-DECLARED-TYPE` (V; blocked 9, finished alone 8, lines 10; confidence high)

- **Root cause:** getTypeOfNode's IsTypeDeclarationName arm answers getDeclaredTypeOfSymbol(alias) unconditionally (error -> printed any) for the name of a type-only ImportClause/ImportSpecifier/ExportSpecifier. TSR's arm covers only specifiers, and when the declared type is error it falls through to the value type.
- **tsgo:** internal/checker/checker.go:31975 getTypeOfNode (IsTypeDeclarationName arm; ast/utilities.go:3585 IsTypeDeclaration) -> checker.go:24094 getDeclaredTypeOfAlias
- **TSR:** crates/tsr-conformance/src/types_producer.rs:818 type_id_at_location_tracking (type-only specifier arm)
- **Examples:**
  - `conformance/importClause_namedImports`: `C`. want `any`, got `""`. `import type { C }` where C is a const: the declared type of the alias target is errorType, which the import-name guard prints as any. TSR's :836 `if declared != error` falls through to the declaration-name arm (:842) and prints the value type (oracle p2.ts: tsgo `C : any`, TSR `C : ""`).
  - `conformance/importClause_default`: `A`. want `A`, got `typeof A`. `import type A from` — IsTypeDeclaration(ImportClause) is true when the clause is type-only, so the answer is the declared type A. TSR's arm matches only Import/ExportSpecifier parents, so the clause name gets get_type_of_symbol (oracle p2.ts: `Dflt : Dflt` vs `typeof Dflt`).
- **Port plan:** Rewrite the arm at types_producer.rs:818 as a port of IsTypeDeclarationName. The parent is an ImportClause whose phase_modifier is `type`, or an Import/ExportSpecifier whose grandparent clause or export declaration is type-only, and the id is the declaration's name. Return checker.get_declared_type_of_symbol(resolve_alias(symbol)) with no fallthrough on error; the render guard at :1888 already prints error as any for import/export statement names. Pitfalls: the ImportClause case must also exclude `import defer`. A specifier's property_name is not its name and must not match. Keep the arm ahead of the declaration-name arm at :842. Once the DECLARED-TYPE-OF-ALIAS arm exists, resolve_alias can go and the alias symbol can be passed directly.
- **Finished alone (8):** conformance/computedPropertyName, conformance/decoratorMetadataWithTypeOnlyImport2, conformance/exportDeclaration_value, conformance/exportDefault, conformance/exportNamespace2, conformance/importClause_default, conformance/importClause_namedImports, conformance/importDefaultNamedType2
- **Also blocked (other clusters needed) (1):** conformance/filterNamespace_import (+QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE)

### 14. `SIGNATURE-DECLARATION-ANNOTATION-REUSE` (V; blocked 16, finished alone 7, lines 62; confidence high)

- **Root cause:** When printing a signature's parameter/return type, upstream serializeTypeForDeclaration/serializeReturnTypeForSignature reuse the WRITTEN annotation node whenever the pseudochecker's type for it is equivalent to the symbol's type (pseudoTypeEquivalentToType, incl. 'error type => charitably equal'); TSR instead consults written_annotation_text, a whitelist of shape heuristics (alias kinds, conditional, infer, same-set unions) and otherwise prints the resolved type.
- **tsgo:** internal/checker/nodebuilderimpl.go:2181 serializeTypeForDeclaration (reuse arm :2231-2256; equivalence gate pseudotypenodebuilder.go:362 pseudoTypeEquivalentToType; returns via nodebuilderimpl.go:2023 serializeReturnTypeForSignature)
- **TSR:** crates/tsr-checker/src/signatures.rs:4945 written_annotation_text (callers: parameter_of :4863, return :1565)
- **Examples:**
  - `conformance/stringLiteralTypesAsTags03`: `hasKind`. want `{ (entity: Entity, kind: "A" \| "A"): entity is A; ... }`, got `{ (entity: Entity, kind: "A"): entity is A; ... }`. `kind: "A" \| "A"` resolves to "A", which equals the param type, so tsgo reuses the written union node. TSR has no union arm for this (its §137 gate only admits same-set reorderings), so it prints the resolved `"A"`; oracle probe `declare function p1(kind: "A" \| "A")` gives the same diff.
  - `compiler/objectCreate2`: `Object.create`. want `{ (o: object \| null): any; ... }`, got `{ (o: object): any; ... }`. With strict:false, lib `o: object \| null` resolves to `object`, which equals the param type, so tsgo reuses the written node. TSR prints the resolved type; oracle probe rb.ts `declare function o(o: object \| null)` shows `(o: object \| null)` vs TSR `(o: object)`.
  - `conformance/typeParameterConstModifiersReverseMappedTypes`: `test1`. want `<const T>(obj: { [K in keyof T]: T[K]; }) => [T, typeof obj]`, got `<const T>(obj: { [K in keyof T]: T[K]; }) => [T, { [K in keyof T]: T[K]; }]`. serializeReturnTypeForSignature reuses the written return `[T, typeof obj]` because its pseudo-type is equivalent to the return type. written_return (signatures.rs:1565) comes from written_annotation_text, which has no tuple/typeof arm; oracle probe r1 confirms.
- **Port plan:** Replace written_annotation_text's heuristic arms with the upstream gate. Reuse the written node when getTypeFromTypeNode(annotation), i.e. pseudoTypeToType, equals the declared/symbol type, or when the type is errorType. For optional params, compare after removing undefined (getTypeWithFacts NEUndefined), and for unions use compareTypesIdentical, as in pseudoTypeEquivalentToType. Emit the reused node through a faithful tryReuseExistingNodeHelper: keep source text, parentheses (`(cond)`) and quotes (`T['type']`). Do this for Parameter.written_text (identifier and binding-pattern roads in parameter_of) and Signature.written_return. Delete the shape-specific arms and the §959 all-rests special case they subsume. Pitfalls: (1) instantiate_signature already clears written text when the type changes (inference.rs:5862-5876), which correctly models the 'not equivalent after instantiation' fallback; keep that. (2) §137 recorded a −270 regression from a blanket reuse that lacked the equivalence check (promiseTypeStrictNull). The gate must be real type equality, not 'annotation exists'. (3) Inferred-return signatures need the pseudochecker's predicate check (pseudoReturnTypeMatchesPredicate).
- **Finished alone (7):** compiler/circularContextualReturnType, compiler/inferenceOptionalPropertiesToIndexSignatures, compiler/objectCreate2, compiler/spreadObjectNoCircular1, conformance/parserUsingConstructorAsIdentifier, conformance/typeParameterConstModifiersReverseMappedTypes [type-operators], conformance/useObjectValuesAndEntries4
- **Also blocked (other clusters needed) (9):** compiler/awaitedTypeJQuery (+AWAITED-THENABLE-EVAL); compiler/complicatedIndexesOfIntersectionsAreInferencable (+CONTEXTUAL-DEFERRED-CONDITIONAL, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); compiler/contextuallyTypedSymbolNamedProperties (+CONTEXTUAL-COMPUTED-SYMBOL-KEY); compiler/declarationEmitMappedTypePreservesTypeParameterConstraint [type-operators] (+DEFERRED-TYPE-REFERENCE-ALIAS, INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, RELATER-NONPRIMITIVE-INDEX-SIGNATURE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, unaligned); compiler/deeplyNestedMappedTypes (+CONDITIONAL-TYPE-ROOT-ALIAS, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/doYouNeedToChangeYourTargetLibraryES2015 (+ARRAY-LITERAL-TUPLE-CONTEXT, CHOOSE-OVERLOAD-CONTEXT-SENSITIVE-ARG-RETENTION, CONDITIONAL-INLINE-NODE-INSTANTIATION, INFER-NO-CANDIDATE-GUARD); compiler/jsxPartialSpread (+SYNTHETIC-DEFAULT-IMPORT-TARGET); compiler/unionOfFunctionAndSignatureIsCallable (+UNTYPED-CALL-FUNCTION-TYPED-CALLEE); conformance/stringLiteralTypesAsTags03 (+NODEBUILDER-GET-REDUCED-TYPE)

### 15. `FORIN-VARIABLE-INDEX-TYPE` (V; blocked 9, finished alone 7, lines 53; confidence high)

- **Root cause:** The for-in arm of getTypeForVariableLikeDeclaration checks the iterated expression and answers getExtractStringType(getIndexType(nonNullable(exprType))) when that index type is a TypeParameter/Index type (and inherits circularity -> any). TSR returns `string` unconditionally without checking the expression.
- **tsgo:** internal/checker/checker.go:16658 getTypeForVariableLikeDeclaration (ForInStatement arm, -> checker.go:26709 getExtractStringType)
- **TSR:** crates/tsr-checker/src/symbols.rs:5174 get_type_for_variable_like_declaration (for-in arm returns intrinsics.string at :5179)
- **Examples:**
  - `compiler/forInStatement3`: `a`. want `Extract<keyof T, string>`, got `string`. `for (var a in expr)` with expr: T; getIndexType(T) is `keyof T` (Index flag) so tsgo wraps it in Extract<_, string>. TSR's arm at symbols.rs:5179 returns string before ever checking the expression.
  - `conformance/parserForOfStatement19`: `of`. want `any`, got `string`. `for (var of in of)`: tsgo checks the expression `of`, which resolves the variable being declared -> circularity -> any. TSR never checks the expression, so there is no cycle and it answers string.
  - `conformance/keyofAndForIn`: `k2`. want `Extract<K, string>`, got `string`. obj: { [P in K]: T }; getIndexType of the generic mapped type is its constraint K (TypeParameter) -> Extract<K, string>. Downstream obj[k2] / x2 gap because the key is string (probe /tmp/triage/v_G11/m.ts confirms `for (const k in o: T)` -> string).
- **Port plan:** Replace the unconditional `string` at symbols.rs:5174-5180 with the upstream arm: check the for-in expression (check_expression, keeping the arm ahead of the annotation), apply getNonNullableTypeIfNeeded, compute getIndexType (TSR has resolved_keyof_type in declared.rs:7546 for concrete types but must produce a deferred `keyof T` Index type for generics / the mapped type's constraint for generic mapped types), and when the result has TypeParameter|Index flags return getExtractStringType (no TSR function exists: port checker.go:26709 — instantiate the global `Extract` alias with [indexType, string] so it prints `Extract<keyof T, string>`), else string. Circularity must flow through the existing resolution stack so `for (var of in of)` reports any. Pitfall: the element-access lines (obj[k], result[k] = ...) in isomorphicMappedTypeInference/mappedTypes4/typeGuardsTypeParameters additionally need indexed access with an Extract<...> conditional index to produce T[Extract<keyof T, string>] (deferred indexed access) and `value & string` narrowing; re-measure them after the arm lands.
- **Finished alone (7):** compiler/forInStatement3, compiler/implicitAnyInCatch, compiler/inOperatorWithGeneric, conformance/keyofAndForIn, conformance/parserForOfStatement19, conformance/parserForOfStatement20, conformance/typeGuardsTypeParameters
- **Also blocked (other clusters needed) (2):** conformance/isomorphicMappedTypeInference (+INFERENCE-REVERSE-MAPPED-MEMBER, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, OBJLIT-PROPERTY-SYMBOL-TYPE); conformance/mappedTypes4 (+COMPARE-TYPES-ARMS, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS)

### 16. `YIELD-STAR-ITERATION-TYPES-OF-ITERABLE` (V; blocked 8, finished alone 7, lines 21; confidence high)

- **Root cause:** TSR has no port of getIterationTypesOfIterable returning all three iteration types (yield/return/next); checkYieldExpression's yield* arm and checkAndAggregateYieldOperandTypes' yield* arm are replaced by shape tests (non-degenerate Array only, plus a Generator-family arm that never fires), so yield* over any other iterable is error and in annotated/contextual generators the expression answers the container's NEXT slot instead of the delegate's RETURN type.
- **tsgo:** internal/checker/checker.go:10952 checkYieldExpression (yield* return arm :10998-11001 -> :6238 getIterationTypeOfIterable -> :6265 getIterationTypesOfIterable / :6357 getIterationTypesOfIterableFast / :6460 getIterationTypesOfIterableSlow / :6541 getIterationTypesOfMethod); aggregation :20322 checkAndAggregateYieldOperandTypes
- **TSR:** crates/tsr-checker/src/expressions.rs:3456 check_yield_expression (yield* arms 3526-3563, 3575-3654); crates/tsr-checker/src/signatures.rs:2497 return_type_from_body (generator yield* arm 2600-2667); engine to generalise: crates/tsr-checker/src/symbols.rs:4620 for_of_yield_types / 4763 semantic_iterable_yield_types
- **Examples:**
  - `compiler/yieldStarContextualType`: `yield* g()  (in `const x2: number = yield* g()`, f(): Generator<string, void, unknown>)`. want `number`, got `unknown`. tsgo answers getIterationTypeOfIterable(YieldStar, Return, Generator<string, number, unknown>) = number. TSR's annotated branch (expressions.rs:3526) only special-cases arrays and then returns next_type_of_annotated_generator = the container's TNext `unknown`; ad-hoc probe a1 (non-generic h(): Generator<string, number, unknown>) gives the same unknown vs number.
  - `conformance/YieldStarExpression4_es6`: `yield * []`. want `any`, got `error`. Under @strict:false the delegate is undefined[] whose ArrayIterator TReturn is BuiltinIteratorReturn = any (strictBuiltinIteratorReturn off). TSR's array arm hard-codes `undefined` and declines the 'degenerate element' as a guessed proxy for this; probe a9 (@strict:true, `yield* []`) shows tsgo `undefined` and `Generator<never, void, unknown>` where TSR gives error, so the element-based decline is
  - `compiler/genericCallAtYieldExpressionInGenericCall2`: `yield* offer(queue, value)`. want `any`, got `error`. Effect's [Symbol.iterator]() returns `{ next(...): IteratorResult<any, any> }`; getIterationTypesOfIterableSlow/getIterationTypesOfMethod give return any and yield any, so the generator is Generator<any, void, any>. TSR's yield* arm only matches Array/Generator-family references (the family arm uses global_type_symbol, which is hard-coded to arity 1, so it never fires for arity-3 Generator), so bo
- **Port plan:** Port IterationTypes{yield,return,next} plus getIterationTypesOfIterable(type, use) for sync uses: getReducedType, any -> {any,any,any}, union -> per-constituent combine (combineIterationTypes), getIterationTypesOfIterableFast (Iterable/IteratorObject/IterableIterator/Generator references -> type args 0/1/2 with lib defaults; ArrayIterator/MapIterator/SetIterator/StringIterator -> (T, BuiltinIteratorReturn, unknown)), getIterationTypesOfIterableSlow ([Symbol.iterator]() call-signature returns -> getIterationTypesOfIterator -> getIterationTypesOfMethod next/return/throw incl. the next-parameter NEXT type and getIterationTypesOfIteratorResult's done-split). Build it by generalising symbols.rs semantic_iterable_yield_types_worker (it already does the slow walk but discards return/next) and have for_of_yield_types read the yield slot from it so for-of/spread share the engine. Then transcribe checkYieldExpression's order: check operand; non-generator -> any; yield* -> getIterationTypeOfIterable(use, Return) orElse any, BEFORE any annotation/contextual lookup; delete the array `undefined` arms (expressions.rs 3545-3563, 3588-3610) and the dead Generator-family arm (3632-3653). In return_type_from_body replace the yield* arm (2600-2667) with checkAndAggregateYieldOperandTypes: yield slot = getYieldedTypeOfYieldExpression (checkIteratedTypeOrElementType(YieldStar)), next slot = iterationTypes.nextType of the delegate. Pitfalls: BuiltinIteratorReturn must come from the lib alias (already modelled in declared.rs:6096), not a hard-coded undefined; preserve Ok/Err 'decidably absent vs unsupported' distinction so unported members still gap rather than turning wrong; recursion guard (resolving_iteration_types) needed for object-literal [Symbol.iterator] generators (generatorTypeCheck46); genericCallAtYieldExpressionInGenericCall3 additionally needs inference through the generic `gen` call once the yield* types exist.
- **Finished alone (7):** compiler/genericCallAtYieldExpressionInGenericCall2, compiler/genericCallAtYieldExpressionInGenericCall3, compiler/yieldStarContextualType, conformance/YieldStarExpression4_es6, conformance/generatorTypeCheck25, conformance/generatorTypeCheck28, conformance/generatorTypeCheck46
- **Also blocked (other clusters needed) (1):** conformance/generatorTypeCheck27 (+CONTEXTUAL-RETURN-IIFE-ARM)

### 17. `MODULE-AUGMENTATION-MERGE` (V; blocked 9, finished alone 6, lines 73; confidence high)

- **Root cause:** mergeModuleAugmentation (non-global arm) is not ported: `declare module "x"` augmentations in module files (and nested ones in ambient modules) are never merged into resolveExternalModuleSymbol(mainModule); TSR only merges global augmentations in the binder.
- **tsgo:** internal/checker/checker.go:1397 mergeModuleAugmentation (non-global arm :1408-1448)
- **TSR:** crates/tsr-binder/src/binder.rs:653 merge_globals (only global augmentations; non-global listed as not merged in merge_symbol doc :803, merge_symbol at :819)
- **Examples:**
  - `compiler/moduleAugmentationExtendAmbientModule1`: `x.map`. want `<U>(proj: (e: number) => U) => Observable<U>`, got `any`. map.ts augments ambient "observable" with `interface Observable<T> { map... }`; tsgo merges the augmentation symbol into the ambient module so main.ts's Observable<number> has map. TSR keeps the augmentation separate -> property missing -> any (probe p5.ts: imported augmentation export `w` is any).
  - `compiler/moduleAugmentationDuringSyntheticDefaultCheck`: `strftime`. want `{ (pattern: string): string; (pattern: string): string; }`, got `(pattern: string) => string`. Both `declare module "moment"` and `declare module "moment-timezone"` (export= of moment) augmentations merge into moment's Moment interface via resolveExternalModuleSymbol, giving two overloads; TSR sees only the local one.
  - `compiler/jsxNamespacedNameNotComparedToNonMatchingIndexSignature`: `"react"`. want `typeof import("react")`, got `error`. Moved here from AMBIENT-MODULE-VALUEMODULE: the file is a module, so `declare module "react" {interface...}` is an external augmentation (declareModuleSymbol -> NamespaceModule, correct in TSR); its name types only because tsgo merges it into react16's ValueModule. Probe p3/p4 `"r"` augmentation: TSR error, oracle typeof import("r").
- **Port plan:** Port the second loop of initializeChecker for non-global augmentations after all files are bound: for each module augmentation (first declaration of its symbol), resolve the name (resolveExternalModuleNameWorker, isForAugmentation), follow export= via resolve_external_module_symbol, and if the target has NAMESPACE flags merge the augmentation symbol into it with the existing binder merge_symbol (pattern-ambient targets: unidirectional; export* targets: merge into resolved exports). Because module resolution lives in the checker/host, this must run where both are available (checker init) or the binder must be given resolved augmentation targets from tsr-compiler's loader (it already resolves module_augmentations, loader.rs:933). Pitfalls: merged_symbol redirects must make the augmentation name resolve to the merged module (name line); moduleAugmentationDuringSyntheticDefaultCheck [3:4] additionally needs the `{ default: ... }` namespace wrapper (ESM-NAMESPACE-SYNTHETIC-DEFAULT-WRAPPER/existing callable clone); module_augmentUninstantiatedModule "foo" merges into export= `var M` (namespace+var) and wants any.
- **Finished alone (6):** compiler/moduleAugmentationDeclarationEmit1, compiler/moduleAugmentationDeclarationEmit2, compiler/moduleAugmentationExtendAmbientModule1, compiler/moduleAugmentationExtendAmbientModule2, compiler/moduleAugmentationExtendFileModule1, compiler/moduleAugmentationExtendFileModule2
- **Also blocked (other clusters needed) (3):** compiler/jsxNamespacedNameNotComparedToNonMatchingIndexSignature (+GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE); compiler/moduleAugmentationDuringSyntheticDefaultCheck (+RESOLVE-ES-MODULE-SYMBOL-CLONE); compiler/module_augmentUninstantiatedModule (+AMBIENT-MODULE-BIND-VALUEMODULE)

### 18. `APPEND-LOCAL-TYPE-PARAMETERS` (V; blocked 9, finished alone 6, lines 32; confidence high)

- **Root cause:** getLocalTypeParametersOfClassOrInterfaceOrTypeAlias/appendTypeParameters collect type parameters from EVERY class/interface/alias declaration of a merged symbol, deduped by merged type-parameter symbol (AppendIfUnique) with defaults read from any declaration of that symbol; TSR's local_type_parameters_of returns the AST list of declarations[0] only, and signatures build one parameter per node.
- **tsgo:** internal/checker/checker.go:23810 appendLocalTypeParametersOfClassOrInterfaceOrTypeAlias (+ :23822 appendTypeParameters, :22007 getResolvedTypeParameterDefault)
- **TSR:** crates/tsr-checker/src/declared.rs:7922 local_type_parameters_of (also signatures.rs:5754 type_parameter_of for function type-param lists)
- **Examples:**
  - `conformance/constructSignaturesWithOverloads2`: `i2`. want `I<number>`, got `any`. `interface I {..}` then `interface I<T> {..}`; upstream appends T from the 2nd decl. TSR reads declarations.first() (no params) so I<number> is an arity error; probe tp.ts `i : error`.
  - `conformance/generatorYieldContextualType`: `StepResult`. want `StepResult<T>`, got `StepResult`. namespace+`type StepResult<T>` merge: first decl is the namespace -> TSR finds no params; probe `namespace NS{} type NS<T>=..; NS<number>` -> error.
  - `conformance/typesWithDuplicateTypeParameters`: `C`. want `C<T>`, got `C<T, T>`. Both T merge into one binder symbol and AppendIfUnique drops the duplicate; TSR creates one per node (probe `class D<T,T>` -> D<T, T>).
- **Port plan:** Replace local_type_parameters_of's first-declaration AST read with a port of appendLocalTypeParametersOfClassOrInterfaceOrTypeAlias: iterate all declarations of kind Class/ClassExpression/Interface/TypeAlias (incl. JSDoc typedef), map each TypeParameterDeclaration to its merged binder symbol and append-if-unique, returning symbols/types instead of AST nodes; callers (local_type_parameter_names_of, declared-type param types, arity/min-arg count) must consume that. Default/constraint lookups must use FirstNonNil over the type-param symbol's declarations (getResolvedTypeParameterDefault) — this fixes jsxCallbackWithDestructuring (class Component<P,S> + interface Component<P={},S={}>: default lives on the 2nd decl; probe `Cm<number>` -> error). Apply the same symbol dedupe in signatures.rs type-parameter list building for `function f<T,T>`. Pitfall: names alone are not identity (class Quux<T> + interface Quux<U> must yield 2 params).
- **Finished alone (6):** compiler/getAccessorWithImpliedReturnTypeAndFunctionClassMerge, compiler/nonIdenticalTypeConstraints, compiler/recursiveGenericMethodCall [contextual], conformance/constructSignaturesWithOverloads2, conformance/mappedTypes2, conformance/typesWithDuplicateTypeParameters
- **Also blocked (other clusters needed) (3):** compiler/jsxCallbackWithDestructuring (+CLASS-GET-BASE-TYPES); conformance/generatorYieldContextualType (+CONDITIONAL-BRANCH-NAMED-UNION, CONTEXTUAL-RETURN-GENERATOR-FILTER, GENERATOR-ANNOTATION-ITERATION-TYPES, INDEXED-ACCESS-GENERIC-DEFER, ORIGIN-ENTRY-ORDER, REMOVE-SUBTYPES-UNDECIDABLE, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE, unaligned); conformance/interfaceExtendsObjectIntersection (+CLASS-GET-BASE-TYPES, DEFERRED-TYPE-REFERENCE-ALIAS)

### 19. `DEFERRED-TYPE-REFERENCE-ALIAS` (V; blocked 9, finished alone 6, lines 27; confidence high)

- **Root cause:** isDeferredTypeReferenceNode's alias arm is not ported for class/interface references and array type nodes: a reference/array node that is directly a type-alias body must become createDeferredTypeReference carrying getAliasForTypeNode's alias, but TSR builds the plain reference (`I<any>`, `string[]`); only the tuple sibling (§79.1) does it.
- **tsgo:** internal/checker/checker.go:23236 isDeferredTypeReferenceNode (alias arm 23237) <- :23200 getTypeFromClassOrInterfaceReference and :24121 getTypeFromArrayOrTupleTypeNode -> :25121 createDeferredTypeReference
- **TSR:** crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference (class/interface/Array<T>/ReadonlyArray<T> targets, final create_type_reference at :4377) and crates/tsr-checker/src/declared.rs:2905 get_type_from_array_type_node (create_type_reference at :2920)
- **Examples:**
  - `compiler/instanceofTypeAliasToGenericClass`: `Table (0:2)`. want `Table`, got `TableClass<any>`. `export type Table = TableClass` (class with `<S = any>`, no args): getAliasSymbolForTypeNode != nil so isDeferredTypeReferenceNode is true and the deferred reference carries alias Table; o/fn lines downstream. TSR fills defaults and prints TableClass<any> (typeVariableConstraintedToAliasNotAssignableToUnion is the same fixture shape, not an independent witness).
  - `conformance/readonlyArraysAndTuples2`: `T10 (0:0)`. want `T10`, got `string[]`. `type T10 = string[]`: getTypeFromArrayOrTupleTypeNode takes the deferred arm (24121) with alias T10; T11 = Array<string> (interface ref) and T12/T13 readonly forms likewise. TSR's get_type_from_array_type_node never consults alias_symbol_for_type_node.
  - `compiler/constraintOfRecursivelyMappedTypeWithConditionalIsResolvable`: `ImmutableTypes (0:0)`. want `ImmutableTypes`, got `IImmutableMap<any>`. `export type ImmutableTypes = IImmutableMap<any>` (interface ref) -> deferred reference with alias. Oracle probe confirms `type A2 = I<number>`, `type A3 = C` (defaulted class), `type D1 = Map<string, number>`, `type D2 = Promise<number>`, `type D3 = ReadonlyArray<string>`, `type A8 = number[]` all print the alias name in tsgo and the expansion in TSR.
- **Port plan:** Port the alias arm of isDeferredTypeReferenceNode exactly as §79.1 already does for tuples (declared.rs:2965-3062): in get_type_from_array_type_node and in get_instantiated_type_reference when the target is a class/interface (not TYPE_ALIAS), if alias_symbol_for_type_node(node) is Some and the alias has no local type parameters (a generic alias is minted nominally upstream-equivalently by get_declared_type_of_type_alias), mint a named copy printing the alias name and carrying the SAME semantics as the plain reference: type_reference_targets=(target,args) for members/instantiation, member owner = target symbol, and whatever array identity TSR keys on (is-array checks, element type, readonly target) so iteration/spread/relations still see an Array. Unlike the type-alias arm there is no body-kind dependence: createDeferredTypeReference always takes the alias. The alias host walk must stay transparent through parentheses and `readonly` (already in type_alias_host_for_type_node), so `type T12 = readonly string[]` names T12. The mint must be cached per node/alias (upstream caches in typeNodeLinks.resolvedType) so repeated reads of the alias return one TypeId. Pitfall: the existing tuple arm and this one should share one helper rather than a third copy of the metadata cloning.
- **Finished alone (6):** compiler/constraintOfRecursivelyMappedTypeWithConditionalIsResolvable, compiler/instanceofTypeAliasToGenericClass, compiler/selfReferencingTypeReferenceInference, compiler/spreadBooleanRespectsFreshness, compiler/typeVariableConstraintedToAliasNotAssignableToUnion, conformance/readonlyArraysAndTuples2
- **Also blocked (other clusters needed) (3):** compiler/declarationEmitMappedTypePreservesTypeParameterConstraint [type-operators] (+INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, RELATER-NONPRIMITIVE-INDEX-SIGNATURE, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, unaligned); compiler/declarationsForIndirectTypeAliasReference (+RESOLVE-ALIAS-INDIRECTION); conformance/interfaceExtendsObjectIntersection (+APPEND-LOCAL-TYPE-PARAMETERS, CLASS-GET-BASE-TYPES)

### 20. `CONTEXTUAL-SIGNATURE-TYPEPARAM-ADOPTION` (V; blocked 7, finished alone 6, lines 30; confidence high)

- **Root cause:** assignContextualParameterTypes copies a GENERIC contextual signature's typeParameters onto the context-sensitive function's signature (sig.typeParameters = context.typeParameters); TSR never adopts them, so the function prints `(x: T) => R` (when the gate lets it through) or error (when the grounded gate sees T in the params).
- **tsgo:** internal/checker/checker.go:10349 assignContextualParameterTypes (adoption at :10350-10356; caller :10152 contextuallyCheckFunctionExpressionOrObjectLiteralMethod)
- **TSR:** crates/tsr-checker/src/signatures.rs:1325 get_signature_from_declaration (type params only from own syntax) + signatures.rs:6201 get_type_of_function_expression grounded gate
- **Examples:**
  - `conformance/genericFunctionParameters`: `x => x`. want `<S>(x: S) => S`, got `(x: S) => S`. f1(cb: <S>(x: S) => T) gives a generic contextual signature; tsgo copies <S> onto the arrow so inference from the generic source erases S (T=unknown). TSR's signature has no type params, so S leaks into T (probe: x1 : S).
  - `compiler/contextualOuterTypeParameters`: `test`. want `<T>(t: T) => void`, got `(t: T) => void`. `const fn2: <T>(x: T) => void = function test(t) {}`; same adoption missing; probe shows symbol `test : (t: T) => void` while the expression itself answers error via the grounded gate.
  - `compiler/genericTypeAssertions3`: `(x) => { return null; }`. want `<T>(x: T) => any`, got `error`. Assertion to <T>(x:T)=>T is a generic contextual signature; param T trips TSR's mentions_any_type_parameter gate, and even ungated it would lack <T>.
- **Port plan:** In the signature TSR builds for a context-sensitive function expression/object-literal method, when the materialised contextual signature (contextual.rs contextual_signature / contextual_signature_result) has type parameters and the own signature has none, set the signature's type_parameters to the contextual signature's (UNinstantiated, same TypeIds) before assigning parameter types, exactly as checker.go:10350-10356; parameter types then reference those params. Then the grounded gate (signatures.rs:6201) must treat params whose type parameters are owned by the adopted list as grounded. Pitfall: inference from a generic source signature (genericFunctionParameters) needs getErasedSignature/instantiate-with-constraint in inferFromSignatures, otherwise T still gets S; the printer must emit <T> on arrow/method types (`{ f<T>(t: T): void; }`). Coordinate with CONTEXTUAL-TYPEPARAM-GROUNDED-GATE: most members here also hit that gate.
- **Finished alone (6):** compiler/contextualTypingWithGenericSignature, compiler/genericFunctionHasFreshTypeArgs, compiler/genericTypeAssertions3, compiler/implicitAnyGenericTypeInference, conformance/genericContextualTypes1, conformance/genericFunctionParameters
- **Also blocked (other clusters needed) (1):** compiler/contextualOuterTypeParameters (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT)

### 21. `CLASS-EXPRESSION-SELF-NAME-RESOLVE` (V; blocked 6, finished alone 6, lines 46; confidence high)

- **Root cause:** NameResolver's ClassExpression arm that resolves a named class expression's own name (meaning includes Class) to the class-expression symbol inside its body is not ported in TSR's binder resolve_name walk.
- **tsgo:** internal/binder/nameresolver.go:189 NameResolver.Resolve (IsClassExpression self-name arm under case KindClassExpression :170)
- **TSR:** crates/tsr-binder/src/lib.rs:730 resolve_name_excluding_with_export_alias (Class/Interface member-lookup arm; add self-name check after it, mirroring the FunctionExpression arm at :722)
- **Examples:**
  - `compiler/classExpressionWithStaticProperties1`: `C.a`. want `number`, got `any`. `var v = class C { static a = 1; static c = C.a + C.b }`: C in the body resolves via nameresolver.go:189 to the class-expression symbol; TSR's walk has only the FunctionExpression self-name arm so C is unresolved -> any. Probe cx.ts `static q = Foo.p` reproduces (Foo : any).
  - `compiler/classBlockScoping`: `new Foo()`. want `Foo`, got `error`. `Foo = class Foo { static y = new Foo() }`: inner Foo must bind to the class expression; TSR misses it (probe: `new Foo()` inside method -> any/error).
- **Port plan:** In the resolve walk, after the class/interface members lookup misses, if the node is a ClassExpression with a name equal to `name` and meaning intersects SymbolFlags::CLASS, return the class expression's own symbol (self.symbol_of(node)), honoring the `exclude` rule as the FunctionExpression arm does. Pitfall: arm must sit after the type-parameter members lookup (type params shadow) and only for ClassExpression, not ClassDeclaration; check calls.rs:2143 / §30 guard that special-cases the miss and delete it once resolved.
- **Finished alone (6):** compiler/classBlockScoping, compiler/classExpressionWithStaticProperties1, compiler/classExpressionWithStaticPropertiesES61, compiler/classExpressionWithStaticPropertiesES62, compiler/classExpressionWithStaticPropertiesES63, conformance/classStaticBlock27

### 22. `OBJLIT-THIS-LITERAL-SELF-FALLBACK` (V; blocked 6, finished alone 6, lines 16; confidence high)

- **Root cause:** getContextualThisParameterType's object-literal arm (noImplicitThis or JS) falls back to getWidenedType(checkExpressionCached(containingLiteral)) when the literal has no contextual type; TSR's contextual_object_this_type returns None on `get_contextual_type(literal)?`, yielding `this`/any.
- **tsgo:** internal/checker/checker.go:12021 getContextualThisParameterType (fallback :12049-12054)
- **TSR:** crates/tsr-checker/src/contextual.rs:611 contextual_object_this_type (early return at :631)
- **Examples:**
  - `conformance/contextualThisTypeInJavascript`: `this`. want `{ prop: number; method(): void; }`, got `any`. JS `const obj = {prop:2, method(){ this }}` uncontextualised; tsgo widens the literal's own type; TSR bails at the `?` (probe TS `{ m(){return this} }` gives `this`).
  - `conformance/typeOfThisInAccessor`: `this`. want `{ readonly a: number; }`, got `any`. Same fallback for a getter in an object literal (probe `{ get a(){return this} }` -> this : any).
- **Port plan:** In contextual_object_this_type, replace `let contextual = self.get_contextual_type(literal)?` with: if Some -> existing ThisType<T> walk, else if no ThisType found return widened(contextual non-nullable) ; if None -> return get_widened_type(check_expression_cached(literal)). Pitfall: circularity — checking the literal while computing `this` inside one of its methods; upstream relies on checkExpressionCached deferring method bodies, TSR must not check the method body when typing the literal. declarationEmitThisPredicates02 are near-duplicates (one witness).
- **Finished alone (6):** compiler/jsPropertyAssignedAfterMethodDeclaration_nonError, compiler/thislessFunctionsNotContextSensitive2, conformance/contextualThisTypeInJavascript, conformance/declarationEmitThisPredicates02, conformance/declarationEmitThisPredicatesWithPrivateName02, conformance/typeOfThisInAccessor

### 23. `SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS` (V; blocked 22, finished alone 5, lines 63; confidence high)

- **Root cause:** getAccessibleSymbolChain's per-table trySymbolTable walk (innermost scope first; direct/ExportSymbol hit sorted with alias candidates by compareSymbols) is not ported for same-file `import x = Entity` aliases: TSR best_name excludes them from the whole printed name (admit_local_import_equals=false), and qualified_name_at/own_name_alias_at require flags==ALIAS so a merged alias+var is skipped.
- **tsgo:** internal/checker/symbolaccessibility.go:535 trySymbolTable (via :373 getAccessibleSymbolChain, nodebuilderimpl.go:1087 getSymbolChain)
- **TSR:** crates/tsr-checker/src/checker.rs:3537 best_name (admit_local_import_equals gate at :3619); also :3725 own_name_alias_at / :2443 qualified_name_at for the type-meaning X.Y case
- **Examples:**
  - `conformance/importStatements`: `a`. want `typeof a`, got `typeof A`. `namespace C { import a = A; var m: typeof a }`: C's locals table holds alias a->A and no direct A, so trySymbolTable returns [a]. TSR best_name's gate skips the import-equals alias and falls to the outer table's A.
  - `probe /tmp/triage/v_G16/a.ts`: `m`. want `typeof a`, got `typeof A`. Oracle-verified: same shape prints typeof a in tsgo; control `namespace M1 { export namespace P{} import pp = P; q = pp }` prints typeof P in both because the exported P's local carries ExportSymbol and sorts before pp (trySymbolTable :554 + compareSymbolChains) — that is what protects the privacy* baselines, not the gate.
  - `conformance/shadowedInternalModule`: `Y`. want `Y`, got `X.Y`. `namespace Z { import Y = X.Y; var Y = 12 }`: the alias Y (merged with var) in Z's locals names class X.Y; TSR qualifies because own_name_alias_at requires flags == ALIAS exactly.
- **Port plan:** Replace best_name's admit_local_import_equals exclusion with faithful trySymbolTable ordering: per table, (1) direct own-name hit returns immediately only when it is accessible as the symbol itself; (2) a local whose ExportSymbol is the target becomes a 1-element candidate competing with alias candidates; (3) aliases (incl. same-file import-equals with Identifier/QualifiedName refs) become candidates; sort with compareSymbolChains (length, then compareSymbols by declaration position). Then drop the bool parameter from all callers. Route qualified_name_at/own_name_alias_at/alias_in_scope_for through the same walk (merged alias+var symbols must count as aliases). Pitfalls: the privacy* families (130 lines previously lost) depend on step (2); globalThisDeclarationEmit3 additionally needs the GLOBALTHIS-SYMBOL fix; internalImport*/reboundIdentifier also need namespace-meaning RHS resolution (WRITER-DECLARED-TYPE-ARMS) for other lines.
- **Finished alone (5):** compiler/constEnumOnlyModuleMerging, compiler/importInTypePosition, compiler/moduleAliasInterface, compiler/moduleCrashBug1, compiler/unusedImports10
- **Also blocked (other clusters needed) (17):** compiler/globalThisDeclarationEmit3 (+GLOBALTHIS-SYMBOL-IN-GLOBALS); compiler/importAliasAnExternalModuleInsideAnInternalModule (+IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET); compiler/internalImportInstantiatedModuleMergedWithClassNotReferencingInstance (+RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM); compiler/internalImportInstantiatedModuleMergedWithClassNotReferencingInstanceNoConflict (+RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM); compiler/internalImportInstantiatedModuleNotReferencingInstance (+RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM); compiler/internalImportUnInstantiatedModuleMergedWithClassNotReferencingInstance (+RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM); compiler/internalImportUnInstantiatedModuleMergedWithClassNotReferencingInstanceNoConflict (+RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM); compiler/moduleVisibilityTest3 (+TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL); compiler/noCrashOnImportShadowing (+RESOLVE-ALIAS-INDIRECTION, TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL); compiler/privacyGloImport (+AMBIENT-MODULE-BIND-VALUEMODULE, BINDER-ALIAS-EXPORT-CONTEXT); compiler/reboundIdentifierOnImportAlias (+RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM); compiler/returnTypeParameterWithModules (+FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS); compiler/usingModuleWithExportImportInValuePosition (+GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE); conformance/circularImportAlias (+RESOLVE-NAME-EXPORTED-IMPORT-EQUALS); conformance/exportImportAlias (+GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE); conformance/importStatements (+QUALIFIED-NAME-LEFT-ALIAS-RESOLVE); conformance/shadowedInternalModule (+DECLARED-TYPE-OF-ALIAS, RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM)

### 24. `QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE` (V; blocked 14, finished alone 5, lines 67; confidence high)

- **Root cause:** After a qualified name resolves, tsgo answers getTypeReferenceType(symbol): getTypeFromTypeAliasReference gives the alias's declared type or instantiation (intrinsic bodies print `number`/`string`/`unique symbol`, generic/conditional bodies are instantiated and evaluated), and getTypeFromClassOrInterfaceReference fills default type arguments (fillMissingTypeArguments). TSR's qualified_type_reference instead mints a Named type spelled with the written text: it has no alias body and no defaults, and an arity window narrower than [min, max] sends `N.G<string>` to the error mint.
- **tsgo:** internal/checker/checker.go:23146 getTypeReferenceType (-> :23580 getTypeFromTypeAliasReference / :23169 getTypeFromClassOrInterfaceReference, :21954 fillMissingTypeArguments)
- **TSR:** crates/tsr-checker/src/declared.rs:4651 qualified_type_reference (argument-less mint :4741-4762, generic mint :4768-4818, fall-to-unresolved :4819)
- **Examples:**
  - `compiler/classFunctionMerging`: `b`. want `number`, got `Foo.Inst`. `declare function Foo(x:number): Foo.Inst` with `namespace Foo { export type Inst = number }`: tsgo's getTypeFromTypeAliasReference returns the declared type `number`. TSR mints Named("Foo.Inst"); probe q1 `declare var a: N.T` (T = string): tsgo string, TSR N.T.
  - `compiler/reactSFCAndFunctionResolvable`: `Checkbox`. want `React.SFC<{}>`, got `React.SFC`. `declare const Checkbox: React.SFC` where `type SFC<P = {}>`: tsgo instantiates the alias with its default (oracle r6: `sfc : React.SFC<{}>`, `cc : React.ComponentClass<{}, any>`). TSR's bare arm mints the written text with no defaults. Probe q1 same-file `N.G<string>` gives tsgo `N.G<string, number>`, while TSR falls to unresolved (`len != parameters`) and so `N.G<string> \| number` is error.
  - `compiler/conditionalTypeRelaxingConstraintAssignability`: `undefined as ElChildren.Void`. want `undefined`, got `ElChildren.Void`. `export namespace ElChildren { export type Void = undefined }` is referenced qualified; tsgo's declared type is the intrinsic `undefined`, so `new Elem(...)` infers Elem<undefined>. TSR's opaque mint flows into inference as Elem<ElChildren.Void>.
- **Port plan:** Route the resolved qualified symbol through the same getTypeReferenceType road the identifier arm already gets right (declared.rs:1620-1651: get_declared_type_of_symbol / get_regular_type_of_literal_type for non-generic, get_instantiated_type_reference with §136/§890 default filling and alias evaluation for generic). Extract that tail into one get_type_reference_type(node, symbol) and call it from both arms. Delete the written-text mints at declared.rs:4741-4818, but keep §280's enum-member arm. Probe u1.ts shows the unqualified road prints `G<string, number>`, `(e: string) => void` for F<string>, `1` for C<"a">, `string` for alias T. Alias-rooted members (React.*, ns.*, lambdaTester.*, JSXInternal.*) also need QUALIFIED-NAME-ALIAS-LEFT-RESOLVE first. Pitfalls: the mint exists because printing a real interface/alias needs the qualifier (getSymbolChain / needsQualification). TSR's printer qualifies interfaces from outside the namespace (probe q4 `r : N.I`) but not type aliases (`s : O`, tsgo `N.O`), so alias instantiations such as `React.SFC<{}>` and `lambdaTester.VerifierFn<any>` need the NB-SYMBOL-CHAIN qualification for aliases. The §933.1/§926 written-spelling channel (qualified_written_text) must keep bare written annotations bare in reused signature prints (jsxNamespaceGlobalReexport's `JSXInternal.HTMLAttributes & ...` vs props `JSXInternal.HTMLAttributes<{}>`). Remeasure the design-W numbers recorded on qualified_type_reference (1,702/20) before deleting.
- **Finished alone (5):** compiler/classFunctionMerging, compiler/conditionalTypeRelaxingConstraintAssignability, compiler/importedAliasedConditionalTypeInstantiation, compiler/moduleVisibilityTest4, compiler/reactSFCAndFunctionResolvable
- **Also blocked (other clusters needed) (9):** compiler/callsOnComplexSignatures (+PRINT-ALIAS-DEFAULT-TYPEARGS, QUALIFIED-NAME-LEFT-ALIAS-RESOLVE, SYNTHETIC-DEFAULT-IMPORT-TARGET); compiler/contextuallyTypedJsxChildren (+SYNTHETIC-DEFAULT-IMPORT-TARGET); compiler/jsxComplexSignatureHasApplicabilityError (+CLASS-GET-BASE-TYPES, QUALIFIED-NAME-LEFT-ALIAS-RESOLVE, unaligned); compiler/jsxNamespaceGlobalReexportMissingAliasTarget (+QUALIFIED-NAME-LEFT-ALIAS-RESOLVE); compiler/reactHOCSpreadprops (+CLASS-GET-BASE-TYPES, SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER); compiler/reactReadonlyHOCAssignabilityReal (+CLASS-GET-BASE-TYPES, JSX-ATTR-CONTEXTUAL-LITERAL, SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER); compiler/typeNamedUndefined1 (+RETURN-WIDEN-UNIQUE-SYMBOL); compiler/typeNamedUndefined2 (+IDENT-UNDEFINED-SHADOWED, RETURN-WIDEN-UNIQUE-SYMBOL); conformance/filterNamespace_import (+TYPE-ONLY-IMPORT-EXPORT-NAME-DECLARED-TYPE)

### 25. `TYPE-ALIAS-ACCESSIBILITY-GATE` (V; blocked 8, finished alone 5, lines 53; confidence high)

- **Root cause:** typeToTypeNode's alias arm uses an alias name only when IsTypeSymbolAccessible(alias, enclosingDeclaration); TSR decides alias naming at type creation with a syntactic local-scope check that is skipped for generic alias instantiations and mapped-type aliases, and never checks cross-file export visibility or orphan duplicate-alias symbols.
- **tsgo:** internal/checker/nodebuilderimpl.go:3362 typeToTypeNode alias arm -> symbolaccessibility.go:11 IsTypeSymbolAccessible
- **TSR:** crates/tsr-checker/src/declared.rs:3594 alias_symbol_for_type_node / :1165 alias_declaration_is_locally_scoped (+ printing at checker.rs:1859 type_to_string_at)
- **Examples:**
  - `conformance/genericTypeAliases`: `x`. want `A[] \| { x: A[] \| any; }`, got `Foo<A[]>`. Function-local generic `type Foo<T>` is not accessible from the print site; tsgo expands. Probe c.ts: non-generic local aliases already expand in TSR, generic local and local mapped-type aliases (P1/P2) keep names.
  - `compiler/inlineMappedTypeModifierDeclarationEmit`: `test2`. want `<T, K extends string>(obj: T, k: K) => { [P in Exclude<keyof T, K>]: T[P]; }`, got `... => OmitUnveiled<T, K>`. Non-exported OmitUnveiled from other.ts is inaccessible in index.ts; TSR has no print-site accessibility check.
  - `conformance/typeAliasesForObjectTypes`: `T2`. want `{ y: number; }`, got `T2`. Duplicate `type T2` second declaration gets an orphan symbol not in any table, so inaccessible -> structural; TSR names it (probe d.ts).
- **Port plan:** Move the alias-name decision to print time: in type_to_string_at, when a type carries an alias, call a ported isSymbolAccessible(alias, reference, Type) (hasVisibleDeclarations + accessible chain) and otherwise print the structural type (recursion -> any via visited set). Then remove the syntactic gates in alias_symbol_for_type_node/alias_declaration_is_locally_scoped. Pitfall: TSR bakes alias text into mints (generic references, mapped, conditional written text), so expanded printing needs the structural printer for those types; binder must model the orphan duplicate-alias symbol.
- **Finished alone (5):** compiler/declarationEmitInferredTypeAlias4, compiler/declarationEmitNestedAnonymousMappedType [type-operators], compiler/inlineMappedTypeModifierDeclarationEmit [type-operators], compiler/mappedTypeGenericInstantiationPreservesHomomorphism [type-operators], conformance/typeAliasesForObjectTypes
- **Also blocked (other clusters needed) (3):** compiler/declarationEmitMappedTypeDistributivityPreservesConstraints [type-operators] (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/jsFileImportPreservedWhenUsed (+INDEXED-ACCESS-NEVER-INDEX, MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS); conformance/genericTypeAliases (+INTERFACE-BASE-FROM-TYPE-NODE)

### 26. `TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL` (V; blocked 8, finished alone 5, lines 16; confidence high)

- **Root cause:** When a type reference's name binds to an alias whose target is unknownSymbol (missing module, unexported member) or has no Type meaning (module, instantiated namespace, `import * as`), tsgo's resolveTypeReferenceName fails and mints getUnresolvedSymbolForEntityName. The result is an errorType carrying an alias that prints the written name (`a`, `WinJS`, `MyPromise<number>`) in error baselines. getSymbol's alias meaning filter skips non-Type targets, and getSymbolFlags reports All for unknown targets so resolveAlias returns unknownSymbol. TSR's alias arms answer plain errorType (printed any) instead.
- **tsgo:** internal/checker/checker.go:23087 resolveTypeReferenceName (-> :23102 getUnresolvedSymbolForEntityName; meaning filter getSymbol :2176, getSymbolFlagsEx unknown->All :16378)
- **TSR:** crates/tsr-checker/src/declared.rs:1397 get_type_from_type_reference alias arms: §157 `return error` :1504, non-TYPE target `return error` :1606, module_specifier_unfindable gate :1608-1615, NamespaceImport/other aliases falling to the declared-type tail :1620
- **Examples:**
  - `conformance/asyncAwaitIsolatedModules_es6`: `mp`. want `MyPromise<number>`, got `any`. `import { MyPromise } from "missing"; declare var mp: MyPromise<number>`: the alias resolves to unknownSymbol, so tsgo mints the unresolved symbol and prints `MyPromise<number>`. TSR's alias road hits module_specifier_unfindable -> error (declared.rs:1615); verdictdump 0:2 WRONG any.
  - `compiler/moduleInTypePosition1`: `w1`. want `WinJS`, got `any`. `import WinJS = require('./m0')` (a module, no Type meaning) used as `w1: WinJS`: getSymbol's meaning filter drops the alias, resolveEntityName gives nil, and the written name is printed. TSR's §157 arm resolves the target, sees no TYPE flag and returns error (declared.rs:1504).
  - `compiler/importDeclWithDeclareModifier`: `b`. want `a`, got `any`. `declare export import a = x.c` where c is not exported: resolveAlias(a) is unknownSymbol, so the unresolved symbol `a` is printed. TSR's §157 target is None -> error.
- **Port plan:** Give get_type_from_type_reference upstream's resolution order. First resolve the name with getSymbol's alias meaning filter: an alias counts only if getSymbolFlags(target) intersects TYPE, and an unknown target counts as All. Then follow aliases (resolveEntityName :15821). Any outcome that is nil or unknownSymbol goes to unresolved_type_reference (declared.rs:4424, which already mints the written-name is_error type). Concretely, replace `return error` at :1504 and :1606, drop the module_specifier_unfindable -> error branch at :1608-1615 (keep unresolved_type_reference), and stop NamespaceImport/ImportEquals aliases with non-type targets from reaching the declared-type tail at :1620. Pitfalls: the types writer prints the alias name only when the file has an error baseline (type_symbol_baseline.go:380). Without one, IsTypeAny prints `error`, so types_producer must keep that gate. unresolved_type_reference already declines a type argument that renders `error`. Re-measure the §491/§493 cycle gate (circular2) since it shares the road.
- **Finished alone (5):** compiler/importDeclWithDeclareModifier, compiler/isolatedDeclarationErrorTypes1, compiler/moduleInTypePosition1, conformance/asyncAwaitIsolatedModules_es2017, conformance/asyncAwaitIsolatedModules_es6
- **Also blocked (other clusters needed) (3):** compiler/moduleVisibilityTest3 (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); compiler/noCrashOnImportShadowing (+RESOLVE-ALIAS-INDIRECTION, SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); compiler/unusedInvalidTypeArguments (+CLASS-GET-BASE-TYPES)

### 27. `RESOLVE-ES-MODULE-SYMBOL-CLONE` (V; blocked 7, finished alone 5, lines 23; confidence high)

Merged from wave-2 clusters: `ESM-NAMESPACE-SYNTHETIC-DEFAULT-WRAPPER`, `ES-NAMESPACE-IMPORT-MODULE-CLONE` (G12-SYNTHETIC-DEFAULT-MODULES, G16-NB-NAMING). They share the same upstream operation and TSR site.

- **Root cause:** resolveESModuleSymbol's namespace-import wrapping is ported only for the hasSignatures class/function arm (module_clone_type); the isEsmCjsRef / `default`-property arms that build `{ default: T }` via getTypeWithSyntheticDefaultImportType (structured: spread) or createDefaultPropertyWrapperForModule (non-structured) are missing, as is the per-file ESM-imports-CJS format test. / resolveESModuleSymbol clones the module symbol per `import * as ns` (cloneTypeAsModuleType) when the module type has signatures OR a `default` property; TSR's module_clone_type only clones callable/class targets, so all aliases share one module type and print the first alias's name (and the clone's members are text-baked without the site).
- **tsgo:** internal/checker/checker.go:15568 resolveESModuleSymbol (arm :15609-15618), :15646 getTypeWithSyntheticDefaultImportType, :15707 createDefaultPropertyWrapperForModule ; internal/checker/checker.go:15610 resolveESModuleSymbol (default-property arm -> :15721 cloneTypeAsModuleType)
- **TSR:** crates/tsr-checker/src/symbols.rs:600 module_clone_type (called from get_type_of_alias symbols.rs:409) ; crates/tsr-checker/src/symbols.rs:600 module_clone_type (+ checker.rs:3767 alias_targets_module_clone, :3782 module_clone_name_at)
- **Examples:**
  - `compiler/moduleExportNonStructured`: `exportAny`. want `{ default: any; }`, got `any`. index.mts (ESM) `import * as exportAny from './exportAny.cjs'` where the .d.cts is `export = __: any`; isEsmCjsRef is true and `any` is not structured, so tsgo wraps with createDefaultPropertyWrapperForModule. TSR's module_clone_type only handles class/function targets with signatures and returns the raw export= type.
  - `compiler/nodeNextCjsNamespaceImportDefault2`: `ns.default`. want `typeof d`, got `"string"`. foo.mts `import * as ns from './a.cjs'` (a.cts has explicit `export default 'string'`); isEsmCjsRef -> getTypeWithSyntheticDefaultImportType spreads `{default: <module a>}` over the module type, overriding the real default. TSR returns the plain module so ns.default is the literal export.
  - `conformance/exportsAndImports4-es6`: `c`. want `typeof c`, got `typeof a`. t1 has `export default`, so `import * as c` gets its own cloned module symbol whose only naming alias is c; TSR keeps the original module, named by the earliest alias `a` (import=require).
- **Port plan:** In module_clone_type (or a new resolve_es_module_symbol used by get_type_of_alias for NamespaceImport and by import()), port :15587-15618: getTypeWithSyntheticDefaultOnly (hasDefaultOnly JSON), then if hasSignatures || type has a `default` property || isEsmCjsRef: structured -> getTypeWithSyntheticDefaultImportType (spread of module type with a `{ default: alias->module }` object whose anonymous symbol carries the module declarations), else createDefaultPropertyWrapperForModule; clone as module type. Needs the same ModuleHost implied-format/usage-mode plumbing as SYNTHETIC-DEFAULT-IMPORT-TARGET (do that first). Pitfall: TSR already emits `{ default: () => moment.Moment; }` for function export= via callable_export_properties — fold that into the new path rather than keeping two producers; the `default` member must print the module by alias (`typeof d`).

Extend module_clone_type to the full resolveESModuleSymbol gate: NamespaceImport (or import call) whose target module type has call/construct signatures, a `default` property (getPropertyOfTypeEx skipObjectFunctionPropertyAugment), or isEsmCjsRef; structured types use getTypeWithSyntheticDefaultImportType, others createDefaultPropertyWrapperForModule. Relax alias_targets_module_clone's CLASS|FUNCTION precondition. Do not bake `printed_type` via type_to_string (symbols.rs:644) — print clone members at the site so qualifiers pick `_moment.Moment` (moduleAugmentationDuringSyntheticDefaultCheck 3:0; that case also blocked by MODULE-AUGMENTATION-MERGE).
- **Finished alone (5):** compiler/moduleExportNonStructured, compiler/unusedImports11, compiler/unusedImports12, compiler/unusedImports_entireImportDeclaration, conformance/exportsAndImports4-es6
- **Also blocked (other clusters needed) (2):** compiler/moduleAugmentationDuringSyntheticDefaultCheck (+MODULE-AUGMENTATION-MERGE); compiler/nodeNextCjsNamespaceImportDefault2 (+SYNTHETIC-DEFAULT-IMPORT-TARGET)

### 28. `UNTYPED-CALL-ARGUMENT-ANY-CONTEXT` (V; blocked 7, finished alone 5, lines 22; confidence high)

- **Root cause:** Arguments of an untyped/error call (resolveUntypedCall/resolveErrorCall, super() with any/error super type) are checked while the call's signature is resolvingSignature/anySignature/unknownSignature, so getContextualTypeForArgumentAtIndex -> getTypeAtPosition answers anyType, there is no contextual signature, and context-sensitive functions type with implicit-any params; TSR's contextual argument road has no untyped-call arm so get_type_of_function_expression's gate gaps them.
- **tsgo:** internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex (resolvingSignature/anySignature have no parameters -> relater.go:1757 getTypeAtPosition returns anyType); producers checker.go:9902 resolveUntypedCall, checker.go:8472-8490 super arms of resolveCallExpression
- **TSR:** crates/tsr-checker/src/contextual.rs:2446 contextual_type_for_argument_resolving (no untyped-call arm) and crates/tsr-checker/src/signatures.rs:6149 get_type_of_function_expression gate (argument_context_is_any signatures.rs:6350 / argument_context_parameter signatures.rs:6327 require a single call signature)
- **Examples:**
  - `compiler/fatarrowfunctionsOptionalArgsErrors4`: `(a) => 110`. want `(a: any) => number`, got `any`. `foo` is undeclared -> resolveErrorCall -> resolveUntypedCall checks args; contextual type at each index is any so `a` is implicit any. TSR already answers the call any (§24 unresolved-identifier arm) but argument_context_parameter needs a single call signature, so the gate returns error (printed any under hadErrorBaseline). Probe with WRITTEN `declare const xx:any; xx(a => a)` also gaps, proving 
  - `compiler/superCallFromFunction1`: `value => String(value)`. want `(value: any) => string`, got `any`. super() outside a class: checkSuperExpression is errorType -> resolveCallExpression's super arm -> resolveUntypedCall checks the arrow with any context. TSR prints the super call `void` RIGHT but the arrow argument is gated (no signature for `super`).
  - `compiler/commentsOnObjectLiteral2`: `initialize: function(name) {...}`. want `(name: any) => void`, got `any`. makeClass is unresolved -> resolveErrorCall; the object-literal argument's contextual type is any, its property `initialize` gets contextual type any, so the function expression has no contextual signature. TSR's gate only knows direct call arguments (argument_context_parameter), so the nested function gaps and the literal collapses (probe `xx({ m: function (n) {} })` gaps).
- **Port plan:** Introduce one predicate mirroring resolveCallExpression/resolveNewExpression's routing to anySignature/unknownSignature (super with any or error super type or no extends clause, import call, isErrorType(apparent), isUntypedFunctionCall incl. clusters UNTYPED-CALL-ANY-CALLEE/FUNCTION-TYPED arms, and 'no call signatures' error arm e.g. `nc(z=>z)` with nc:number) and use it at the top of contextual_type_for_argument_resolving to return Some(any) for every index (exactly what resolving_signature_calls already returns at contextual.rs:2435). Then make get_type_of_function_expression's gate accept an any contextual type through the general contextual road (contextual_signature_result -> Absent) instead of the bespoke argument_context_is_any/argument_context_parameter direct-argument walk, so nested positions (object-literal property of an argument: commentsOnObjectLiteral2) inherit 'property of any is any'. Super calls must route here too (is `super(...)` a CallExpression whose callee check yields error? use checkSuperExpression's answer, not single_call_signature). Pitfall: do NOT answer any for calls TSR merely failed to resolve (overload failure/gaps) - only for the upstream untyped/error arms; parser509534's callee `server.get` is any via unannotated param of an assigned function expression, so it also depends on which provenance arms the callee predicate admits - decide args by the upstream routing, not by TSR's call-result gate.
- **Finished alone (5):** compiler/commentsOnObjectLiteral2, compiler/fatarrowfunctionsOptionalArgsErrors4, compiler/superCallFromClassThatHasNoBaseType1, compiler/superCallFromFunction1, conformance/parser509534
- **Also blocked (other clusters needed) (2):** compiler/superCallFromClassThatDerivesFromGenericTypeButWithIncorrectNumberOfTypeArguments1 (+CLASS-GET-BASE-TYPES, SUPER-FROM-BASE-TYPES); compiler/superCallFromClassThatDerivesNonGenericTypeButWithTypeArguments1 (+CLASS-GET-BASE-TYPES)

### 29. `AMBIENT-MODULE-BIND-VALUEMODULE` (V; blocked 7, finished alone 5, lines 9; confidence high)

- **Root cause:** bindModuleDeclaration declares every ambient module that is not an external augmentation (string-named `declare module "x"`, or `declare global` in a script) as ValueModule unconditionally; TSR picks NAMESPACE_MODULE from module_instance_state for all module declarations, so type-only/empty ambient modules have no value type (name line prints error, or any for `global`).
- **tsgo:** internal/binder/binder.go:771 bindModuleDeclaration (ambient arm :773-781)
- **TSR:** crates/tsr-binder/src/binder.rs:4931 symbol-flag table arm Node::ModuleDeclaration
- **Examples:**
  - `conformance/typeReferenceRelatedFiles`: `"fs"`. want `typeof import("fs")`, got `error`. `declare module "fs" { interface FSWatcher {} }` in a script d.ts is NonInstantiated; tsgo binds it ValueModule anyway so getTypeOfFuncClassEnumModule gives the module object. TSR binds NAMESPACE_MODULE -> no value type -> error (probe p4.ts: "n","o","t" all error, oracle typeof import(...)).
  - `compiler/moduleAugmentationGlobal6`: `global`. want `typeof global`, got `any`. `declare global { interface Array<T> {x} }` in a script is IsAmbientModule && !IsModuleAugmentationExternal -> ValueModule; TSR NamespaceModule. Oracle p4.ts confirms `global : typeof global` vs TSR any.
- **Port plan:** In the flag table (binder.rs:4931) take the ambient arm first: if is_ambient_module(node) (string literal name or GlobalKeyword) and NOT is_module_augmentation_external, use S::VALUE_MODULE with ValueModuleExcludes regardless of instance state; keep module_instance_state only for identifier namespaces and external augmentations (declareModuleSymbol). The binder already has is_ambient_module and the IsModuleAugmentationExternal test inside is_merged_global_augmentation (:1162) — factor it out. Pitfall: the §95/§94 excludes note (binder.rs:4914-4919) — ambient ValueModule now occupies value space, so verify no new duplicate-identifier diagnostics; `global` printing must yield `typeof global` (TSR already does this for module-file global blocks).
- **Finished alone (5):** compiler/duplicatePackage_globalMerge, compiler/moduleAugmentationGlobal5, compiler/moduleAugmentationGlobal6, conformance/parserModuleDeclaration2, conformance/typeReferenceRelatedFiles
- **Also blocked (other clusters needed) (2):** compiler/module_augmentUninstantiatedModule (+MODULE-AUGMENTATION-MERGE); compiler/privacyGloImport (+BINDER-ALIAS-EXPORT-CONTEXT, SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 30. `RESOLVE-NAME-EXPORT-DEFAULT-LOCAL` (V; blocked 6, finished alone 5, lines 30; confidence high)

- **Root cause:** NameResolver's module arm first checks moduleExports['default'] and accepts it when GetLocalSymbolForExportDefault(result).Name == name; TSR's binder exports arm has no default-local check, so the local name of `export default class C`/`function f` resolves nowhere.
- **tsgo:** internal/binder/nameresolver.go:113 Resolve (export-default local-name arm, 110-120)
- **TSR:** crates/tsr-binder/src/lib.rs:784 resolve_name_excluding_with_export_alias exports arm (add default arm before 802)
- **Examples:**
  - `conformance/decoratorOnClass3.es6`: `C`. want `typeof C`, got `any`. `export default class C {}; let c = new C()`: C is only under exports['default'] (with its local symbol), so the TSR resolver misses it. Probe p2: `var k = C` gives any.
  - `compiler/exportDefaultAsyncFunction`: `foo`. want `() => Promise<void>`, got `any`. `export default async function foo` then `foo()`: same missing arm. Probe p2 u.ts: `export default function Ex(){}; var e = Ex` gives any.
- **Port plan:** In the SourceFile / ambient-non-global ModuleDeclaration exports arm, look up exports[INTERNAL_DEFAULT] first. If its local symbol for export default (the declaration's local symbol with that name; the binder already sets export_symbol, binder.rs ~L3938) has the name and result.flags & meaning != 0, return it. Mirror the 110 condition, which excludes non-ambient namespaces. This subsumes the classifier keys RESOLVE-DEFAULT-EXPORT-LOCAL and EXPORT-VALUE-LOCAL-TO-EXPORT-SYMBOL for default-export cases.
- **Finished alone (5):** compiler/declarationEmitDefaultExportWithStaticAssignment, compiler/defaultDeclarationEmitNamedCorrectly, compiler/exportDefaultAsyncFunction, conformance/decoratorOnClass3.es6, conformance/decoratorOnClass7.es6
- **Also blocked (other clusters needed) (1):** compiler/exportDefaultAbstractClass (+CLASS-GET-BASE-TYPES)

### 31. `LOGICAL-OR-COALESCE-GENERIC-GATE` (V; blocked 6, finished alone 5, lines 16; confidence high)

- **Root cause:** `a || b` / `a ?? b` build getUnionTypeEx([nonNullable(left), right], UnionReductionSubtype); TSR check_logical_or_coalescing returns error whenever a constituent is a type parameter or unknown (or a reference with such args) instead of running removeSubtypes.
- **tsgo:** internal/checker/checker.go:12509 checkBinaryLikeExpressionWorker BarBar arm (:12518 QuestionQuestion arm) -> :25934 removeSubtypes
- **TSR:** crates/tsr-checker/src/binary.rs:452 check_logical_or_coalescing (undecidable gate at :508-516)
- **Examples:**
  - `compiler/nonNullableTypes1`: `x \|\| "hello"`. want `"hello" \| NonNullable<T>`, got `error`. x: T; left is NonNullable<T> (generic); binary.rs:508 TYPE_PARAMETER gate returns error. Probe `function f<T>(x:T){return x\|\|"h"}` -> f returns any.
  - `conformance/nullishCoalescingOperator2`: `a7 ?? 'whatever'`. want `{}`, got `error`. GetNonNullableType(unknown) = {} ; {} \| 'whatever' reduces to {}. TSR's gate flags UNKNOWN on the pair (probe `a7 ?? 'w'` -> error).
- **Port plan:** Delete the undecidable-flags early return and route these pairs through union_with_subtype_reduction (removeSubtypes) using isTypeStrictSubtypeOf, which must decide type-parameter sources via their constraint (T extends X is a subtype of X; unconstrained T only of unknown/{}?) and `{}`/fresh-empty targets. Pitfall: GetNonNullableType of unknown must produce `{}` (and of T produce NonNullable<T>) before reduction; nullishCoalescingOperator_es2020 is a near-duplicate of nullishCoalescingOperator2.
- **Finished alone (5):** compiler/discriminatedUnionJsxElement, compiler/nonNullableTypes1, conformance/nullishCoalescingOperator2, conformance/nullishCoalescingOperator_es2020, conformance/nullishCoalescingOperator_not_strict
- **Also blocked (other clusters needed) (1):** compiler/typeofImportInstantiationExpression (+GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE, IMPORT-TYPE-NODE-TYPEOF)

### 32. `FUNCEXPR-NIL-CONTEXTUAL-SIGNATURE-POSITIONS` (V; blocked 5, finished alone 5, lines 53; confidence high)

- **Root cause:** Upstream: getContextualSignature nil -> assignNonContextualParameterTypes (implicit any) for any position without a contextual type; TSR instead requires its syntactic has_no_contextual_type whitelist to PROVE absence and errors otherwise, and the whitelist lacks void operand, array-literal element, arrow expression body / return in an uncontextual function or unannotated getter, and parameter initializer.
- **tsgo:** internal/checker/checker.go:10395 assignNonContextualParameterTypes (from :10152 contextuallyCheckFunctionExpressionOrObjectLiteralMethod when getContextualSignature :10264 is nil); getter return nil via :29665 getContextualReturnType/:20058 getReturnTypeFromAnnotation
- **TSR:** crates/tsr-checker/src/signatures.rs:4406 has_no_contextual_type (+ :4055 declaration_takes_no_contextual_return lacks GetAccessor arm) feeding :6149 get_type_of_function_expression
- **Examples:**
  - `compiler/nestedRecursiveLambda`: `r => r`. want `(r: any) => any`, got `error`. `void(r =>(r => r))` and `[(r=>(r=>r))]`: void operand / array element / arrow body of an uncontextual arrow have no contextual type; TSR whitelist has no VoidExpression, ArrayLiteral or arrow-body arm (probe `void (r => r)` -> error).
  - `compiler/contextualTypingOfAccessors`: `(n)=>n`. want `(n: any) => any`, got `error`. `get foo() { return (n)=>n }` with unannotated setter: getReturnTypeFromAnnotation nil, accessors have no contextual signature -> nil; declaration_takes_no_contextual_return answers false for GetAccessor so the gate errors and foo becomes any.
  - `compiler/thisInConstructorParameter2`: `(p = this) => this`. want `(p?: this) => this`, got `error`. Arrow as initializer of unannotated ctor parameter zzz: getContextualTypeForVariableLikeDeclaration -> getContextuallyTypedParameterType nil (ctor not context sensitive); no Parameter arm in has_no_contextual_type (probe `function g(a = (p = 1) => p)` -> error).
- **Port plan:** Long-term: replace the prove-absence whitelist with upstream's direct test (compute getContextualSignature; nil -> assignNonContextualParameterTypes). Minimum port: add arms to has_no_contextual_type for VoidExpression/TypeOf/Delete/Await operands (TSR AST kinds distinct from PrefixUnary), ArrayLiteralExpression element (climb, per checker.go getContextualTypeForElementExpression), ArrowFunction expression body (treat as return -> declaration_takes_no_contextual_return(arrow)), Parameter initializer (getContextuallyTypedParameterType: nil unless containing function is context-sensitive with a contextual signature), and in declaration_takes_no_contextual_return a GetAccessor arm = no own annotation AND paired setter param unannotated. Pitfall: the comment at 4597 records nestedRecursiveLambda going GAP->WRONG when array climb was added alone — the inner arrow's return-type inference must also see the arrow body arm, so add arms together.
- **Finished alone (5):** compiler/contextualTypingOfAccessors, compiler/nestedRecursiveLambda, compiler/thisInConstructorParameter2, conformance/privateNameAccessorsCallExpression, conformance/privateNameStaticAccessorsCallExpression

### 33. `CANDIDATE-FOR-OVERLOAD-FAILURE` (V; blocked 5, finished alone 5, lines 10; confidence high)

- **Root cause:** When chooseOverload rejects every candidate, resolveCall still returns getCandidateForOverloadFailure -> pickLongestCandidateSignature, instantiated by inferSignatureInstantiationForOverloadFailure (getInferredType default + constraint) or by getTypeArgumentsFromNodes for written type args; TSR's call road returns error for generic failures and its new road ignores arity (no constraint fallback) or errors on excess type arguments.
- **tsgo:** internal/checker/checker.go:9498 getCandidateForOverloadFailure (-> :9510 pickLongestCandidateSignature, :9575 inferSignatureInstantiationForOverloadFailure, :9557 getTypeArgumentsFromNodes)
- **TSR:** crates/tsr-checker/src/calls.rs:624 check_call_expression_worker (failure branch L693-729) + crates/tsr-checker/src/expressions.rs:2430 check_new_expression (single candidate L2521, written type args L2703)
- **Examples:**
  - `compiler/couldNotSelectGenericOverload`: `makeArray(1, "")`. want `unknown[]`, got `any`. Arity failure on single generic signature; upstream infers with no usable candidates -> unknown. Oracle probe mk2<T>(items:T[]) mk2(1,'') tsgo unknown[], TSR error (calls.rs:693 only answers non-generic agreeing returns).
  - `conformance/instantiateGenericClassWithWrongNumberOfTypeArguments`: `new C<number, number>()`. want `C<number>`, got `any`. Excess type args fail arity; pickLongestCandidateSignature instantiates with getTypeArgumentsFromNodes (truncated). Oracle probe new C<number,number>() tsgo C<number>, TSR error.
  - `conformance/derivedClassWithoutExplicitConstructor`: `new D()`. want `D<Date>`, got `D<unknown>`. Missing required arg; failure instantiation's getInferredType gives unknown, not assignable to constraint Date -> Date. Probes: new D5() with required param gives D5<unknown> in TSR, optional-param valid call gives D<Date>, so only the arity-failed road lacks the constraint fallback.
- **Port plan:** Port getCandidateForOverloadFailure/pickLongestCandidateSignature/getLongestCandidateIndex/inferSignatureInstantiationForOverloadFailure as one function used by both call and new roads (and choose_overload's tail at calls.rs:2061 which already ports createUnionOfSignaturesForOverloadFailure for non-generic sets). For generic picks: written type args -> getTypeArgumentsFromNodes + fillMissingTypeArguments; else fresh inference context with SkipContextSensitive|SkipGenericFunctions and getInferredTypes (default -> unknown -> constraint). Remove the agreeing-return heuristic at calls.rs:721-729 and make check_new_expression detect arity failure for a single candidate instead of calling check_generic_call_with blindly.
- **Finished alone (5):** compiler/couldNotSelectGenericOverload, compiler/tooManyTypeParameters1, conformance/derivedClassWithoutExplicitConstructor, conformance/derivedClassWithoutExplicitConstructor2, conformance/instantiateGenericClassWithWrongNumberOfTypeArguments

### 34. `LATE-BIND-INDEX-SIGNATURE-TYPE-LITERAL` (V; blocked 5, finished alone 5, lines 6; confidence high)

- **Root cause:** Type-literal members with a computed name follow getResolvedMembersOrExportsOfSymbol's switch: late-bindable name -> real member, late-bindable index signature (entity-name expr assignable to string|number|symbol, incl. any/error) -> aggregated [x: K] index info, anything else -> silently absent; TSR's build_type_literal returns error for computed METHOD signatures and unannotated computed properties, and applies checkObjectLiteral's (non-entity-name) key rule instead of isLateBindableIndexSignature.
- **tsgo:** internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (:19662 hasLateBindableIndexSignature arm; :19976 isLateBindableIndexSignature requires isLateBindableAST :19990), selected by checker.go:15930 getResolvedMembersOrExportsOfSymbol (:15948/:15953 switch)
- **TSR:** crates/tsr-checker/src/declared.rs:1935 build_type_literal (method computed arm :2050-2054 `None => return error`; property computed arm :2305-2343 incl. `let Some(annotation) = property.r#type else { return error }` :2323; aggregation :2437-2477)
- **Examples:**
  - `conformance/parserComputedPropertyName14`: `v`. want `{ [x: number]: () => number; }`, got `any (error)`. `var v: { [e](): number }` with unresolved `e` (error key, entity name) is a late-bindable index signature; error is assignable to number first, giving a number index of the method type. TSR's MethodSignature arm calls late_bound_symbol_member_name, gets None, and returns error for the whole literal (declared.rs:2053); oracle confirms (`v3 : { [x: number]: () => number; }` vs TSR error).
  - `compiler/propertyAssignment`: `foo2`. want `{ [x: number]: any; }`, got `any (error)`. `{ [index]; }` (unresolved index, no annotation) is the same late-bindable index signature with value any; TSR reaches ComputedNameKey::Index("number") but then `let Some(annotation) = property.r#type else { return error }` (declared.rs:2323) rejects the unannotated member. Probe `var v2: { [e] }` -> TSR error, tsgo `{ [x: number]: any; }`.
  - `conformance/computedPropertyNamesDeclarationEmit4_ES6`: `v`. want `{}`, got `any (error)`. `["" + ""](): void` is neither late-bindable name nor late-bindable index signature (not an entity name), so the member is simply not in getMembersOfSymbol and the literal is `{}`. TSR returns error from the method arm; oracle also shows the PROPERTY form `{ [""+""]: number }` is `{}` upstream while TSR prints `{ [x: string]: number; }` (computed_member_index_key lacks the entity-name gate).
- **Port plan:** Replace the computed-name handling in build_type_literal (both MethodSignature and PropertySignature arms) with upstream's three-way dispatch shared with the class cluster: (1) isLateBindableName (entity-name expr + literal/unique-symbol type) -> named member as today; (2) isLateBindableIndexSignature (isLateBindableAST + assignable to string|number|symbol) -> contribute to the index aggregation, value type = the member's type (method signature type; unannotated property = any; `?` adds undefined) — no `return error` for methods or missing annotations; (3) otherwise drop the member (`continue`). Aggregate via getObjectLiteralIndexInfo semantics: value = union over the computed members AND all sibling members matching the key (oracle: `{ [k]: number; y: string }` indexes as `string | number`, TSR says `number`), only when no explicit index info for that key exists (replace the current `indexes.is_empty()` all-or-nothing gate and the `computed_indexes` mixed-key `return error`). Pitfalls: do NOT reuse computed_member_index_key unchanged — it implements checkObjectLiteral's rule (no entity-name gate) which is right for object literals only; the node builder prints index-info components as `[k]: T` properties (oracle: `{ [s]: () => number; }`, `{ [s]?: number | undefined; }` for a declared `s: string`) while error/any-keyed ones print `[x: number]` — check how the printer chooses before changing rendering; enum-typed names (KNOWN DEVIATION comment at :2309) are late-bindable names, not index signatures.
- **Finished alone (5):** compiler/propertyAssignment, conformance/computedPropertyNamesDeclarationEmit4_ES6, conformance/parserComputedPropertyName14, conformance/parserComputedPropertyName18, conformance/parserComputedPropertyName19

### 35. `RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM` (V; blocked 11, finished alone 4, lines 11; confidence high)

- **Root cause:** getTypeOfNode's isInRightSideOfImportOrExportAssignment arm resolves every entity-name part with getSymbolAtLocation: Namespace meaning and dontResolveAlias for a bare identifier or qualified-name left of `import x = …`, the export-assignment symbol for `export =`/`export default`. It answers declared-then-value. TSR has no arm for a bare-identifier import= RHS (it falls to value resolution, which picks shadowing locals), and its export-assignment arm gates on a TYPE-vs-VALUE resolve_name equality heuristic.
- **tsgo:** internal/checker/checker.go:32016 getTypeOfNode (isInRightSideOfImportOrExportAssignment arm; utilities.go:1107) + checker.go:14474 getSymbolOfPartOfRightHandSideOfImportEquals
- **TSR:** crates/tsr-conformance/src/types_producer.rs:1244 type_id_at_location_tracking (import-equals qualified-left arm; no bare-identifier arm) and :1147 (export-assignment arm)
- **Examples:**
  - `compiler/internalImportUnInstantiatedModuleNotReferencingInstanceNoConflict`: `A`. want `error`, got `number`. In `import Y = A` beside a local `var A = 1`, the bare A is resolved with Namespace meaning to an uninstantiated namespace, whose declared and value types are both error. TSR has no bare-identifier import= arm, so the final check_expression resolves the shadowing var (oracle p1.ts with `namespace B { var A = 1; import Y = A; }`: tsgo `typeof Y`, TSR `number`).
  - `conformance/importAliasIdentifiers`: `clodule`. want `clodule`, got `typeof clodule`. `import clolias = clodule` (class+namespace): the arm's getDeclaredTypeOfSymbol is the class instance type, which is not error, so it wins. TSR answers the value type through the expression path (oracle p1.ts `import Cl = C1`: tsgo `C1`, TSR `typeof C1`).
  - `compiler/reservedNameOnModuleImportWithInterface`: `mi_string`. want `mi_string`, got `any`. Interface plus empty namespace on the RHS: Namespace-meaning resolution finds the merged symbol, and its declared type is the interface. TSR's value path finds nothing (oracle p1.ts `import Ii = I`: tsgo `I`, TSR error).
- **Port plan:** Port isInRightSideOfImportOrExportAssignment as one arm, placed after the declaration-name and binding arms as upstream orders them. For an ImportEquals moduleReference, resolve with getSymbolOfPartOfRightHandSideOfImportEquals' meanings: a bare identifier or qualified left gets Namespace, a whole qualified name gets Value|Type|Namespace, and aliases are not resolved. For ExportAssignment, use the export-assignment expression's symbol (the local symbol, including binder export-value markers merged with a local interface, as in defaultNamedExportWithType3). Answer get_declared_type_of_symbol and fall back to get_type_of_symbol when it is error. This replaces three pieces of TSR code: the :1244 qualified-left special case, the :1285 leaf special case, and the :1147 export arm with its resolve_name TYPE/VALUE equality heuristic. It also adds the bare-identifier case, which is missing today. Pitfalls: binder.resolve_name is not meaning-filtered against locals the way resolveEntityName is, so a shadowing `var A` must be skipped for Namespace meaning. Several want strings (`Y`, `typeof Y`, `typeof Point`) also need NB-SYMBOL-CHAIN's alias-name printing, so lines from the internalImport* and reboundIdentifier cases only turn RIGHT after both land. The `export = Math` (local namespace vs global interface) behaviour that the heuristic protected must come out of correct resolution.
- **Finished alone (4):** compiler/defaultNamedExportWithType3, compiler/internalImportUnInstantiatedModuleNotReferencingInstanceNoConflict, compiler/reservedNameOnModuleImportWithInterface, conformance/importAliasIdentifiers
- **Also blocked (other clusters needed) (7):** compiler/internalImportInstantiatedModuleMergedWithClassNotReferencingInstance (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); compiler/internalImportInstantiatedModuleMergedWithClassNotReferencingInstanceNoConflict (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); compiler/internalImportInstantiatedModuleNotReferencingInstance (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); compiler/internalImportUnInstantiatedModuleMergedWithClassNotReferencingInstance (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); compiler/internalImportUnInstantiatedModuleMergedWithClassNotReferencingInstanceNoConflict (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); compiler/reboundIdentifierOnImportAlias (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS); conformance/shadowedInternalModule (+DECLARED-TYPE-OF-ALIAS, SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 36. `CONTEXTUAL-RETURN-IIFE-ARM` (V; blocked 6, finished alone 4, lines 61; confidence high)

- **Root cause:** getContextualReturnType's last arm returns getContextualType(iife call) for an immediately invoked function; TSR's ReturnStatement arm (and yield operand path) stops at contextual_signature(function)? with no IIFE fallback.
- **tsgo:** internal/checker/checker.go:29690 getContextualReturnType (iife arm)
- **TSR:** crates/tsr-checker/src/contextual.rs:1430 get_contextual_type ReturnStatement arm (+ yield-operand contextual return path); immediately_invoked_call exists at contextual.rs:401
- **Examples:**
  - `compiler/contextualReturnTypeOfIIFE3`: `arg`. want `number`, got `any`. `app.foo.bar = (function(){ return { someFun(arg){} } })()`: return context = contextual type of the call = typeof app.foo.bar; TSR has no IIFE arm (probe with typed const: arg any).
  - `conformance/generatorTypeCheck64`: `x => x.length`. want `(x: string) => number`, got `error`. yield operand of generator IIFE takes the generator's contextual return from the call's contextual type Iterable<(x:string)=>number>; missing arm -> arrow gated to error (probe `const v: Iterable<...> = function*(){yield x=>x.length}()` -> error).
- **Port plan:** After the annotation and contextual-signature branches in the ReturnStatement arm (contextual.rs:1417-1431) and in the generator yield-operand contextual return computation, add: if immediately_invoked_call(function) is Some(call) return get_contextual_type(call) (passed through contextual_return_expression_slot for generators/async). Also the get_type_of_function_expression gate must then see the arrow as contextual (no change needed if contextual_signature resolves). Pitfall: immediately_invoked_call must walk parentheses like ast.GetImmediatelyInvokedFunctionExpression; avoid recursion when the IIFE call's contextual type depends on its own return.
- **Finished alone (4):** compiler/contextualReturnTypeOfIIFE3, conformance/generatorTypeCheck29, conformance/generatorTypeCheck30, conformance/generatorTypeCheck64
- **Also blocked (other clusters needed) (2):** conformance/generatorTypeCheck27 (+YIELD-STAR-ITERATION-TYPES-OF-ITERABLE); conformance/types.asyncGenerators.es2018.1 (+ASYNC-YIELD-STAR-ITERATION-TYPES, GENERATOR-ANNOTATION-ITERATION-TYPES)

### 37. `EMPTY-NAMED-FUNCTION-SYMBOL-TYPE` (V; blocked 6, finished alone 4, lines 6; confidence medium)

- **Root cause:** getTypeOfFuncClassEnumModuleWorker gives a method/function symbol an anonymous object type regardless of its name; TSR's worker gates on has_a_name_no_type_query_can_spell (name.is_empty()) before distinguishing typeof-spelled kinds, so any method whose symbol name is "" (a `[""]`/`""` name or parser-recovered missing name in `*() {}`) types as error.
- **tsgo:** internal/checker/checker.go:16912 getTypeOfFuncClassEnumModuleWorker
- **TSR:** crates/tsr-checker/src/symbols.rs:2732 get_type_of_func_class_enum_module_worker (gate at :2751 via :3030 has_a_name_no_type_query_can_spell)
- **Examples:**
  - `conformance/computedPropertyNames41_ES6`: `[""]`. want `() => Foo`, got `error`. `static [""]() { return new Foo }` late-binds to a method symbol named ""; tsgo builds its function type. TSR's worker returns intrinsics.error at :2758 because the name is empty; probe shows every ""-named method (object literal, class, static, interface, and reads `i1[""]`, `C2[""]`) errors while a ""-named PROPERTY types fine.
  - `conformance/MemberFunctionDeclaration4_es6`: `(empty name of `*() { }`)`. want `() => Generator<never, void, unknown>`, got `any (error)`. Parser recovery leaves an empty Identifier as the method name; the bound symbol is named "" and the declaration-name line takes getTypeOfSymbol, which TSR answers error through the same gate (probe: `class C4 { *() { } }` -> `> : error`).
- **Port plan:** Restrict the empty-name/`__class` gate to the symbols that would actually be spelled by a type query (CLASS — keeping the anonymous_class_written_name arm — ENUM, VALUE_MODULE), i.e. evaluate it after/inside the `typeof` branch at :2780-2791, and let FUNCTION/METHOD symbols fall through to the signature-based anonymous type. The EMPTY-NAME-DECL-LINE key (types_producer declaration-name arm) needs no change: the producer resolves the symbol; the type is what errors. Pitfalls: FunctionPropertyAssignments2/6 lines 0:0/0:1 (printing `""()` member name) belong to NB-PROPERTY-NAME, not here; confirm the missing-name method's binder symbol is named "" (the producer arm at types_producer.rs:841 must find it via symbol_of(parent)).
- **Finished alone (4):** conformance/MemberFunctionDeclaration4_es6, conformance/computedPropertyNames10_ES6, conformance/computedPropertyNames13_ES6, conformance/computedPropertyNames41_ES6
- **Also blocked (other clusters needed) (2):** conformance/FunctionPropertyAssignments2_es6 [contextual] (+CLASSIFY-PROPERTY-NAME); conformance/FunctionPropertyAssignments6_es6 [contextual] (+CLASSIFY-PROPERTY-NAME)

### 38. `MAPPED-MEMBER-KEYS-ENUM-AND-ANY` (V; blocked 5, finished alone 4, lines 26; confidence high)

- **Root cause:** resolveMappedTypeMembers' key enumeration: TSR's mapped_member_keys rejects enum-literal keys (only STRING_LITERAL|NUMBER_LITERAL admitted) and has no `any` modifiers-type arm (forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType adds a string index for any), so the mapped type gets no members.
- **tsgo:** internal/checker/checker.go:20894 resolveMappedTypeMembers; :22727 forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType (any arm :22731)
- **TSR:** crates/tsr-checker/src/mapped.rs:481 mapped_member_keys (L576 literal-flag admission, L587 `return None`; L495-545 homomorphic source without any arm)
- **Examples:**
  - `compiler/numericEnumMappedType`: `b1[1]`. want `string \| undefined`, got `error`. `{[k in E1]?: string}` keys are enum literals → props 0/1/2 upstream. Probe m1.ts: `{[K in E]?: string}` e[E.A] and alias R<E> both error, while literal keys resolve.
  - `compiler/typeGuardNarrowsIndexedAccessOfKnownProperty11`: `m[E.A]`. want `string \| null`, got `error`. Same enum-literal key rejection; the `m` print lines additionally need MAPPED-TYPE-TEXT-MINT-PRINT once members exist.
  - `compiler/reverseMappedTypeRecursiveInference`: `test(bar)`. want `{ [x: string]: any; }`, got `error`. Bar<any> is homomorphic over any; tsgo enumerates a string index key for any (22731). TSR source=any branch yields no keys/index (probe ka: R<keyof any> error).
- **Port plan:** Port resolveMappedTypeMembers' key loop literally: for non-homomorphic constraints iterate getLowerBoundOfKeyType(constraint) constituents and call addMemberForKeyType, which accepts any isTypeUsableAsPropertyName key (string/number/unique-symbol/ENUM literals, property name = literal value) and index-key types; for homomorphic constraints use forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType including `any → stringType`. Replace the flag test at mapped.rs:576 and the never-only fallthrough at :587. typeGuard* `m` print lines also depend on MAPPED-TYPE-TEXT-MINT-PRINT.
- **Finished alone (4):** compiler/numericEnumMappedType, compiler/reverseMappedTypeRecursiveInference, compiler/typeGuardNarrowsIndexedAccessOfKnownProperty11, compiler/typeGuardNarrowsIndexedAccessOfKnownProperty12
- **Also blocked (other clusters needed) (1):** compiler/reducibleIndexedAccessTypes (+GENERIC-REDUCIBLE-INDEXED-ACCESS)

### 39. `IMPORT-TYPE-NODE-TYPE-MEANING` (V; blocked 5, finished alone 4, lines 24; confidence high)

- **Root cause:** getTypeFromImportTypeNode with type meaning is ported only for a qualified reference without type arguments; written type arguments (`import("./id").Id<X>`) and the unqualified form (`import("./foo")` of an export= class) return error.
- **tsgo:** internal/checker/checker.go:24575 getTypeFromImportTypeNode (qualifier walk :24594-24633, unqualified :24634-24644) -> :24657 resolveImportSymbolType -> getTypeReferenceType
- **TSR:** crates/tsr-checker/src/declared.rs:4600 get_type_from_import_type_node
- **Examples:**
  - `compiler/declarationEmitTopLevelNodeFromCrossFile2`: `boxedBox`. want `import("./box").Box<{ boxed: import("./box").Box<number>; }>`, got `any`. boxedBox.d.ts annotates with `import("./box").Box<{...}>`; tsgo resolves via getTypeReferenceType with the written type arguments. TSR returns error on `!node.type_arguments.is_empty()` (probe p7 `import("./id").Id<number>` error vs oracle number).
  - `compiler/declarationImportTypeAliasInferredAndEmittable`: `Conn`. want `import("./foo")`, got `error`. `type Conn = import("./foo")` with foo.ts `export = Conn` (class): no qualifier, resolveExternalModuleSymbol gives the class, getTypeReferenceType. TSR requires a qualifier (probe p7 `d` error vs oracle import("./foo")).
- **Port plan:** Rewrite get_type_from_import_type_node to follow upstream: resolve module, resolve_external_module_symbol (follow export=), walk qualifier through exports with Namespace meaning then target meaning, then resolveImportSymbolType: for type meaning call the existing type-reference path (getTypeReferenceType with the node's type arguments) on the resolved symbol rather than minting text. Pitfall: the current mint prints the written text `import("m").X`; a real reference must still print `import("./box").Box<...>` via the nodebuilder's module-specifier spelling (the `.js` variants in declarationEmitNoInvalidCommentReuse1/2 want reuse of written specifiers — NB-TYPENODE-REUSE); unqualified export= class prints `import("./foo")`.
- **Finished alone (4):** compiler/declarationEmitNoInvalidCommentReuse1, compiler/declarationEmitNoInvalidCommentReuse2, compiler/declarationEmitTopLevelNodeFromCrossFile2, compiler/declarationImportTypeAliasInferredAndEmittable
- **Also blocked (other clusters needed) (1):** compiler/importUsedInGenericImportResolves (+GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE)

### 40. `IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET` (V; blocked 5, finished alone 4, lines 22; confidence high)

- **Root cause:** `import y = x` where x is itself an alias: resolveEntityName(Namespace, dontResolveAlias=true) accepts an alias whose chain carries Namespace meaning and returns it; TSR's Identifier arm rejects ALIAS symbols unless their value type is a module_value_clone of a namespace.
- **tsgo:** internal/checker/checker.go:14474 getSymbolOfPartOfRightHandSideOfImportEquals (via 14439 getTargetOfImportEqualsDeclaration; resolveEntityName 15772)
- **TSR:** crates/tsr-checker/src/symbols.rs:1124 resolve_alias ModuleReference::Identifier arm (ALIAS branch 1148-1165)
- **Examples:**
  - `compiler/aliasInaccessibleModule2`: `X`. want `typeof N`, got `any`. `import R = N; export import X = R;` — R is an alias of namespace N; tsgo returns alias R and resolveIndirectionAlias follows it. Probe `import R = NS; import Y = R` gives Y any in TSR, because the 1148 branch only admits module_value_clones.
  - `compiler/chainedImportAlias`: `y`. want `typeof x`, got `any`. `import y = x` where x = require(...); x is ALIAS with a module-object value that is not a module_value_clone namespace, so the arm returns None.
- **Port plan:** In the Identifier arm, replace the module_value_clones test with upstream's getSymbol meaning check: accept `found` when it has NAMESPACE, or when it is ALIAS and get_symbol_flags(found) (the alias-chain flags) intersects NAMESPACE. Return the alias itself (dontResolveAlias=true); the chain is then followed by get_type_of_alias's existing recursion, or by the indirection step from RESOLVE-ALIAS-INDIRECTION once that lands. Keep the meaning re-check for non-alias locals (`const x; import q = x` must still reject). Pitfall: printing. tsgo prints `typeof x` / `typeof C` through the in-scope alias name, so check NB-SYMBOL-CHAIN naming on importAliasAnExternalModuleInsideAnInternalModule.
- **Finished alone (4):** compiler/aliasInaccessibleModule2, compiler/chainedImportAlias, compiler/declFileForExportedImport, compiler/es6ImportNamedImportInIndirectExportAssignment
- **Also blocked (other clusters needed) (1):** compiler/importAliasAnExternalModuleInsideAnInternalModule (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 41. `TYPE-LITERAL-PROPERTY-ANNOTATION-REUSE` (V; blocked 5, finished alone 4, lines 10; confidence high)

- **Root cause:** addPropertyToElementList serializes each property of a type-literal/interface type through serializeTypeForDeclaration on the PropertySignature, reusing the written annotation node when it is equivalent (or the type is error). TSR's build_type_literal bakes the member's printed text at construction from type_to_string(member_type), with written reuse only for single-member literal / array / reordered-union shapes.
- **tsgo:** internal/checker/nodebuilderimpl.go:2486 addPropertyToElementList -> :2181 serializeTypeForDeclaration
- **TSR:** crates/tsr-checker/src/declared.rs:1935 build_type_literal (member `printed` at :2405)
- **Examples:**
  - `conformance/elementAccessChain.2`: `o3`. want `{ b: undefined \| { c: string; }; }`, got `{ b: { c: string; }; }`. With strict:false, `undefined \| {c: string}` reduces to `{c: string}`, which equals the prop type, so tsgo reuses the written node. TSR prints the reduced type; oracle probe rb.ts reproduces this.
  - `conformance/intersectionTypeInference2`: `b`. want `{ prop: string & number; }`, got `{ prop: never; }`. `string & number` resolves to never, which equals the prop type, so the written node is reused. TSR prints never; oracle probe `tl` shows the same for `b: string & number`, `a: Array<number>` and `c: "A" \| "A"`.
  - `compiler/typeofInObjectLiteralType`: `a`. want `{ b: number; c: typeof b; }`, got `{ b: number; c: any; }`. `typeof b` is an error type (b is not a value in scope). pseudoTypeEquivalentToType charitably returns true for error types, so the written node is reused. TSR prints any.
- **Port plan:** At member print time (not baked at construction), for a property symbol whose declaration is a PropertySignature/PropertyDeclaration with an annotation, apply the serializeTypeForDeclaration gate: reuse the written node if get_type_from_type_node(annotation) equals the property type (with undefined stripped for `?` members), or if the type is error. Otherwise print the type. Remove the TypeLiteral/Array/Union special-casing at declared.rs:2405-2420 and the §930 'unresolved' branch, which the error-type rule subsumes. Nested literals reused verbatim keep computed names (`["a_b_c"]`), per declarationEmitComputedPropertyName1. Pitfall: interface members and mapped/instantiated props (type differs) must still render. Share the gate with SIGNATURE-DECLARATION-ANNOTATION-REUSE and SPREAD-PROPERTY-ANNOTATION-REUSE.
- **Finished alone (4):** compiler/declarationEmitComputedPropertyName1, compiler/typeofInObjectLiteralType, conformance/elementAccessChain.2, conformance/propertyAccessChain.2
- **Also blocked (other clusters needed) (1):** conformance/intersectionTypeInference2 (+INFERENCE-REVERSE-MAPPED-INTERSECTION)

### 42. `BINDING-ELEMENT-ALLOW-MISSING-DEFAULT` (V; blocked 4, finished alone 4, lines 16; confidence high)

- **Root cause:** getBindingElementTypeFromParentType indexes with AccessFlagsAllowMissing when the element has a default, so a property absent from the parent yields undefined and the element becomes the default's type; TSR's destructuring_property_lookup has no allow-missing arm and answers errorType.
- **tsgo:** internal/checker/checker.go:17719 getBindingElementTypeFromParentType (accessFlags AllowMissing when hasDefaultValue; union at :17789)
- **TSR:** crates/tsr-checker/src/destructure.rs:996 destructuring_property_lookup (miss -> intrinsics.error at :1051)
- **Examples:**
  - `conformance/parserForStatement9`: `x`. want `boolean`, got `error`. `let {x = 'a' in {}} = {}`: x absent from `{}`, AllowMissing gives undefined, union with default boolean -> boolean. TSR lookup on `{}` misses property and index -> error; !x / x = !x lines cascade.
  - `conformance/destructuringObjectBindingPatternAndAssignment9SiblingInitializer`: `b1`. want `number`, got `error`. `const { a1, b1 = a1 } = { a1: 1 }`: b1 missing, AllowMissing -> undefined ∪ typeof a1. Probe `const {a1, b1 = 5} = {a1: 1}` gives b1 : error in TSR.
  - `conformance/destructuringWithLiteralInitializers`: `x`. want `number`, got `error`. f7({ a: { x = 0, y = 0 } } = { a: {} }): nested parent `{}` lacks x; probed: x/y error while the same shape with { a: {x:1,y:2} } types number.
- **Port plan:** Thread an allow_missing flag (element.initializer.is_some(), plus upstream's noTupleBoundsCheck caller) into destructuring_property_lookup (or, better, into the getIndexedAccessTypeEx road the computed-name cluster introduces) and, when neither a property nor an applicable index info exists and the parent is an object type, answer undefined instead of error (getPropertyTypeForIndexType's AllowMissing arm). The default-union leg at destructure.rs:288-358 already turns undefined ∪ default into the default type. Pitfall: keep the error for non-default elements (TS2339 road) and do not let AllowMissing swallow errors on primitives/never parents.
- **Finished alone (4):** conformance/destructuringObjectBindingPatternAndAssignment9SiblingInitializer, conformance/destructuringWithLiteralInitializers, conformance/parserForOfStatement25, conformance/parserForStatement9

### 43. `INSTANTIATION-EXPRESSION-TYPE` (V; blocked 4, finished alone 4, lines 14; confidence high)

- **Root cause:** checkExpressionWithTypeArguments/getInstantiationExpressionType (instantiate a value's generic signatures with explicit type args) is unported, both for expression `f<number>` and type query `typeof f<T>`.
- **tsgo:** internal/checker/checker.go:10660 getInstantiationExpressionType (entry :10637 checkExpressionWithTypeArguments; typeof path :24102 getTypeFromTypeQueryNode)
- **TSR:** crates/tsr-checker/src/declared.rs:1220 get_type_from_type_query_node (returns error on type_arguments at :1222) and crates/tsr-checker/src/expressions.rs:606 check_expression_worker (no ExpressionWithTypeArguments arm)
- **Examples:**
  - `compiler/assignmentToInstantiationExpression`: `getValue<number>`. want `() => number`, got `any`. expression with type args -> getInstantiationExpressionType; TSR worker has no arm. Probe inst.ts `const a = g<number>` -> error.
  - `conformance/arrayTypeOfTypeOf`: `xs3`. want `typeof Array<number>`, got `error`. TypeQuery with type args; TSR declared.rs:1222 returns error outright (probe `typeof g<string>` -> error).
- **Port plan:** Port checkExpressionWithTypeArguments + getInstantiationExpressionType (and helpers getInstantiatedTypePart/getInstantiatedSignatures: filter signatures by type-arg arity, getSignatureInstantiation, rebuild anonymous type with instantiationExpressionType object flags so the printer emits `typeof f<T>`). Wire into check_expression_worker for Expression::ExpressionWithTypeArguments and into get_type_from_type_query_node in place of the early error. Pitfall: printer needs the instantiation-expression node to print `typeof Array<number>`; circular `typeof foo<T>` in foo's own return must defer (circularInstantiationExpression wants `(t: string) => any`).
- **Finished alone (4):** compiler/assignmentToInstantiationExpression, compiler/circularInstantiationExpression, compiler/selfReferentialFunctionType, conformance/arrayTypeOfTypeOf

### 44. `EXPORT-ASSIGNMENT-ALIAS-LIKE-EXPRESSION` (V; blocked 4, finished alone 4, lines 13; confidence high)

- **Root cause:** getTargetOfExportAssignment → getTargetOfAliasLikeExpression resolves any ExpressionIsAlias expression (identifier, entity-name property access, class expression); TSR's export_assignment_target and declaration_of_alias_symbol accept only an Identifier expression, so `export default C.x` and `export = class B {}` (binder-flagged ALIAS) have no target.
- **tsgo:** internal/checker/checker.go:14996 getTargetOfAliasLikeExpression (from 14976 getTargetOfExportAssignment)
- **TSR:** crates/tsr-checker/src/symbols.rs:1546 export_assignment_target (+1344 declaration_of_alias_symbol ExportAssignment predicate)
- **Examples:**
  - `compiler/exportDefaultQualifiedNameNoError`: `def`. want `number`, got `any`. `export default C.x` then `import def`. Probe p6 gives def any: the predicate at 1344 skips the non-identifier ExportAssignment, so resolve_alias finds no declaration.
  - `compiler/modulePreserve1`: `B`. want `typeof B`, got `any`. `export = class B {}`: the binder flags ALIAS because ExpressionIsAlias includes ClassExpression (binder.rs:4874, matching ast/utilities.go:1872). The classifier's 'Property' theory is wrong. TSR resolve_alias then has no target. Probe p8: class expression gives any, while object literal (PROPERTY) works.
- **Port plan:** Make declaration_of_alias_symbol accept ExportAssignment exactly when expression_is_alias holds (Identifier, entity-name PropertyAccess, ClassExpression), matching ast.IsAliasSymbolDeclaration. In export_assignment_target, port getTargetOfAliasLikeExpression: ClassExpression → checkExpressionCached(expr).symbol (binder.symbol_of(class)); otherwise resolveEntityName(expr, VALUE|TYPE|NAMESPACE, dontResolveAlias=true), which shares the QUALIFIED-ENTITY-LEFT-ALIAS fix for the left side. The BinaryExpression arm of resolve_alias at 978 has similar ad-hoc PropertyAccess logic and could reuse it. Also update symbols.rs:1420's export= chain walk, which is identifier-only.
- **Finished alone (4):** compiler/exportDefaultProperty2, compiler/exportDefaultQualifiedNameNoError, compiler/exportEqualsProperty2, compiler/modulePreserve1

### 45. `JSDOC-SETTER-PARAM-ACCESSOR-TYPE` (V; blocked 4, finished alone 4, lines 9; confidence high)

- **Root cause:** getTypeOfAccessors/getWriteTypeOfAccessors read the setter's parameter annotation, which in JS is the reparsed JSDoc @param type; TSR's accessor_annotation reads only the written r#type of the setter parameter.
- **tsgo:** internal/checker/checker.go:20106 getAnnotatedAccessorTypeNode (SetAccessor arm -> getEffectiveSetAccessorTypeAnnotationNode; called from getTypeOfAccessors :18511)
- **TSR:** crates/tsr-checker/src/symbols.rs:375 accessor_annotation
- **Examples:**
  - `compiler/declarationEmitClassSetAccessorParamNameInJs`: `bar`. want `string`, got `any`. `/** @param {string} baz */ set bar(baz){}` in foo.js: reparser puts {string} on baz, getTypeOfAccessors reads it. Probe: TSR types param `v : string` (jsdoc_parameter_annotation works) but `bar : any` since accessor_annotation only reads parameters[0].r#type.
  - `compiler/declarationEmitObjectLiteralAccessorsJs1`: `obj4`. want `{ x: number; }`, got `{ x: any; }`. Object-literal `set x(a)` with `@param {number} a`; same setter-annotation read, so read type and the divergent get/set printing of obj2 (`set x(a: number)`) are lost.
- **Port plan:** In accessor_annotation's SetAccessorDeclaration arm, fall back to self.jsdoc_parameter_annotation(first param id) when r#type is None (JS only), and make the write-type path (getWriteTypeOfAccessors equivalent, used for obj2's divergent `{ get x(): string; set x(a: number); }`) use the same helper. Check the object-literal accessor member path in objects.rs uses accessor_annotation too; Js3's `prop` gap is downstream of bar's type.
- **Finished alone (4):** compiler/declarationEmitClassSetAccessorParamNameInJs, compiler/declarationEmitClassSetAccessorParamNameInJs2, compiler/declarationEmitClassSetAccessorParamNameInJs3, compiler/declarationEmitObjectLiteralAccessorsJs1

### 46. `SPREAD-PROPERTY-ANNOTATION-REUSE` (V; blocked 4, finished alone 4, lines 7; confidence high)

- **Root cause:** Spread copies property symbols that keep their original PropertySignature declarations, so printing the spread result goes through serializeTypeForDeclaration on those declarations and reuses the written annotation (an optional member adds no `| undefined`). TSR's spread_properties sets printed_type = type_to_string(value) for non-captured sources and adds undefined for optional members.
- **tsgo:** internal/checker/nodebuilderimpl.go:2486 addPropertyToElementList -> :2181 serializeTypeForDeclaration (:2249 isOptionalAnnotated strip)
- **TSR:** crates/tsr-checker/src/spreads.rs:274 spread_properties (printed_type at :331, optional at :347)
- **Examples:**
  - `conformance/intersectionIncludingPropFromGlobalAugmentation`: `{ ...source }`. want `{ toString: null \| 'string'; }`, got `{ toString: "string" \| null; }`. source: Test1 is an interface whose member is written `null \| 'string'`. The spread prop keeps that declaration, so tsgo reuses the node. TSR renders the type; probe sp.ts with an interface source reproduces this (a type-literal source already matches via baked text).
  - `compiler/declarationEmitComputedPropertyNameEnum1`: `{ ...({} as Type) }`. want `{ x?: { a: 0; }; }`, got `{ x?: { a: 0; } \| undefined; }`. The copied optional `x?` reuses its annotation, since the type is equivalent once undefined is stripped, so no `\| undefined` is printed. TSR prints the optional type; probe sp.ts `{ ...t }` shows the same.
- **Port plan:** Print spread-copied members through the same property serializer as TYPE-LITERAL-PROPERTY-ANNOTATION-REUSE, keyed on property.origin's declaration: reuse the written annotation when equivalent (modulo undefined for optional). Stop computing printed_type eagerly from the value at spreads.rs:331/206/405. Enum2's `{ [Enum.A]: 0 }` (unresolved Enum, error type) then reuses verbatim.
- **Finished alone (4):** compiler/declarationEmitComputedPropertyNameEnum1, compiler/declarationEmitComputedPropertyNameEnum2, compiler/declarationEmitComputedPropertyNameEnum3, conformance/intersectionIncludingPropFromGlobalAugmentation

### 47. `TYPELIT-INDEX-INFO-KEY-DEDUPE` (V; blocked 4, finished alone 4, lines 6; confidence high)

- **Root cause:** getIndexInfosOfIndexSymbol adds an index info for a key type only if findIndexInfo finds none yet, so repeated `[x: number]` signatures yield one; TSR's build_type_literal pushes one rendered index member (and IndexInfo) per declaration.
- **tsgo:** internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (findIndexInfo guard :19655)
- **TSR:** crates/tsr-checker/src/declared.rs:1935 build_type_literal (index arm :2188-2190: `typed_indexes.extend` / `indexes.push(rendered)`)
- **Examples:**
  - `conformance/multipleNumericIndexers`: `a`. want `{ [x: number]: string; }`, got `{ [x: number]: string; [x: number]: string; }`. `var a: { [x: number]: string; [x: number]: string }` — the second info for key number is discarded by findIndexInfo; TSR renders both. multipleStringIndexers is the string-key twin.
  - `conformance/duplicateStringIndexers`: `a`. want `{ [x: string]: string; }`, got `{ [x: string]: string; [x: string]: string; }`. Same rule inside a namespace-scoped type literal; TSR's index_infos_of_symbol (class/interface path) already dedupes by key, only the type-literal builder does not.
- **Port plan:** Build the literal's index infos once via index_infos_of_declaration per signature with the findIndexInfo-by-key guard (first wins, per key of a union key), and render the index rows FROM the deduped infos instead of from per-declaration `index_signature_member` text; or share index_infos_of_symbol's push_from dedupe. Pitfall: a union key `[x: string | number]` splits into two infos — dedupe per resulting key, not per declaration; keep the degenerate `[]` skip at :2185.
- **Finished alone (4):** conformance/duplicateNumericIndexers, conformance/duplicateStringIndexers, conformance/multipleNumericIndexers, conformance/multipleStringIndexers

### 48. `TYPEREF-ARITY-ERRORTYPE-COMPOSES` (V; blocked 4, finished alone 4, lines 5; confidence high)

- **Root cause:** getTypeFromClassOrInterfaceReference returns errorType for a generic referenced with the wrong arity, and that errorType is an ordinary type that composes: `IFoo[]` is `any[]`, `C<I>` is `C<any>`. TSR uses the same intrinsics.error as its 'unported gap' sentinel, so get_instantiated_type_reference and array/composite builders propagate it as a gap and poison the whole annotation.
- **tsgo:** internal/checker/checker.go:23169 getTypeFromClassOrInterfaceReference (errorType at :23197; composed by createTypeReference / :24115 getTypeFromArrayOrTupleTypeNode)
- **TSR:** crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference (arity `return error` :4062; per-argument `if resolved == error { return error }` :4071)
- **Examples:**
  - `compiler/missingTypeArguments2`: `y`. want `A<any>`, got `any`. `class A<T>{}; var y: A<A>`: the inner bare `A` fails arity and becomes errorType, and the outer reference becomes A<errorType>, printed `A<any>`. TSR's inner `A` returns error at :4062, and the outer loop returns error at :4071, printed any.
  - `compiler/genericArrayWithoutTypeAnnotation`: `foo`. want `any[]`, got `any`. `foo: IFoo[]` with generic `interface IFoo<T>`: the element is the arity errorType and the array type is any[]. TSR's error element is a gap and the whole parameter type is error.
- **Port plan:** Separate the definite errorType (upstream answered errorType, as in arity failures from getTypeFromClassOrInterfaceReference / getTypeFromTypeAliasReference :23603 / checkNoTypeArguments) from TSR's gap sentinel. Mint a distinct definite-error intrinsic (ANY-flagged, prints `any`, not in the gap set) at the arity returns (declared.rs:4062, :1626, :1593 and the qualified equivalents). Let get_instantiated_type_reference (:4071), array/tuple and union builders accept it as a normal type argument or element. Pitfalls: is_error is identity-based crate-wide. The new intrinsic must not be is_error for propagation, but it must still suppress follow-on diagnostics the way errorType does upstream. Bare `var i: I` must keep printing `any`.
- **Finished alone (4):** compiler/genericArrayWithoutTypeAnnotation, compiler/genericInterfacesWithoutTypeArguments, compiler/genericTypeReferencesRequireTypeArgs, compiler/missingTypeArguments2

### 49. `NODEBUILDER-GET-REDUCED-TYPE` (V; blocked 7, finished alone 3, lines 20; confidence high)

- **Root cause:** typeToTypeNode calls getReducedType before printing (unions drop never-reduced intersections; intersections with conflicting discriminants print never); TSR's printer never reduces.
- **tsgo:** internal/checker/nodebuilderimpl.go:3229 typeToTypeNode -> checker.go:21819 getReducedType
- **TSR:** crates/tsr-checker/src/checker.rs:1859 type_to_string_at (reuse flow.rs:6409 intersection_has_never_discriminant)
- **Examples:**
  - `compiler/distributiveConditionalTypeNeverIntersection1`: `Conflicted`. want `never`, got `Conflicted`. {x:true}&{x:false} reduces to never in typeToTypeNode before the alias arm.
  - `conformance/stringLiteralTypesAsTags01`: `d`. want `never`, got `A & B`. Narrowed A & B with kind 'A'/'B' discriminants; getReducedType in printer yields never.
- **Port plan:** Port getReducedType (union arm with ContainsIntersections filtering; intersection arm via isNeverReducedProperty = discriminant-with-never or conflicting private) as a checker function, cache it, and apply it at the top of type_to_string_at_worker and on nested constituents. Reuse intersection_has_never_discriminant; add isConflictingPrivateProperty. Pitfall: reduction also matters in relation/property access upstream; this cluster only needs the print call.
- **Finished alone (3):** compiler/distributiveConditionalTypeNeverIntersection1, conformance/stringLiteralTypesAsTags01, conformance/stringLiteralTypesAsTags02
- **Also blocked (other clusters needed) (4):** compiler/genericRestTypes (+FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/identityRelationNeverTypes (+RELATE-CONDITIONAL); compiler/typeVariableConstraintIntersections (+INTERSECTION-TYPEVAR-CONSTRAINT-REDUCTION); conformance/stringLiteralTypesAsTags03 (+SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### 50. `BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS` (V; blocked 5, finished alone 3, lines 23; confidence high)

- **Root cause:** For an object binding element with a computed property name, getBindingElementTypeFromParentType indexes the parent with getIndexedAccessTypeEx(parent, getLiteralTypeFromPropertyName(name)) (literal/unique-symbol keys select the property, generic keys defer to T[K]) and getRestType omits those keys; TSR only consults the parent's applicable index signature for a computed key (and declines it in the rest member list), so property-backed and generic keys gap.
- **tsgo:** internal/checker/checker.go:17707 getBindingElementTypeFromParentType (object arm :17740-17742; rest :17737 -> :17792 getRestType; :26769 getLiteralTypeFromPropertyName)
- **TSR:** crates/tsr-checker/src/destructure.rs:170 get_type_for_binding_element_impl (computed_key arm -> get_applicable_index_info) and destructure.rs:827 object_rest_type
- **Examples:**
  - `compiler/declarationEmitComputedNameCausesImportToBePainted`: `value`. want `string`, got `error`. `({ [Key]: value }: Context) => value` with Key a unique symbol and Context declaring `[Key]: string`; tsgo's literal type of the name is the unique symbol, selecting the property. TSR's computed arm (destructure.rs:186) only asks for an index signature, finds none -> error.
  - `conformance/genericObjectRest`: `a1`. want `T[K1]`, got `error`. `let { [k1]: a1, [k2]: a2, ...r1 } = obj` with k1: K1 extends keyof T; getIndexedAccessTypeEx defers to T[K1] and getRestType yields Omit<T, K1 \| K2>. TSR has no deferred-indexed-access road for a computed key, and r1 also gaps.
  - `compiler/destructuredMaappedTypeIsNotImplicitlyAny`: `bar`. want `{ [_ in T]: number; }[T]`, got `error`. `const { [key]: bar } = obj` with key: T; tsgo indexes the mapped type by T (deferred). TSR's index-info lookup fails -> error.
- **Port plan:** In destructure.rs get_type_for_binding_element_impl, replace the computed_key special case (lines ~170-196) with the upstream sequence for every non-rest object element: index type = getLiteralTypeFromPropertyName(name) (identifier/string/numeric names -> literal; computed -> checkComputedPropertyName type, unique symbols included) and element type = getIndexedAccessTypeEx(parent, index, ExpressionPosition|AllowMissing-if-default) — route through indexed.rs (resolved_indexed_access_type / deferred_indexed_access) rather than destructuring_property_lookup + get_applicable_index_info so generic keys produce T[K] and symbol keys hit properties. Keep §691's index-signature behaviour as a consequence of that call (lateBoundDestructuringImplicitAnyError). For rests, make object_rest_type accept computed names by collecting getLiteralTypeFromPropertyName of each sibling and, when any is generic, build Omit<T, K1|K2> via the global Omit alias as getRestType does (checker.go:17792). Pitfall: the existing literal-default branch (property_name_from_index) becomes subsumed — delete it. renamingDestructuredPropertyInFunctionType 0:84/0:94/0:166 (`string` element of `{ ["a"]: string }: O`) also land here.
- **Finished alone (3):** compiler/destructuredMaappedTypeIsNotImplicitlyAny, conformance/controlFlowBindingElement, conformance/genericObjectRest
- **Also blocked (other clusters needed) (2):** compiler/declarationEmitComputedNameCausesImportToBePainted (+CLONE-BINDING-NAME); compiler/renamingDestructuredPropertyInFunctionType (+BINDING-PATTERN-IMPLIED-TYPE, CLONE-BINDING-NAME)

### 51. `OBJECT-LITERAL-PSEUDOTYPE-REUSE` (V; blocked 5, finished alone 3, lines 18; confidence high)

- **Root cause:** Object-literal property types are serialized via serializeTypeForDeclaration -> pseudochecker (GetTypeOfDeclaration/typeFromExpression; GetTypeOfAccessor). An `as T` / `<T>` assertion yields the asserted type node, `'x' as const` yields the source literal node, and a getter's return annotation is reused, so source spelling survives (single quotes, `IThing<typeof foo>`, `Array<number>`). TSR's check_object_literal_members only reuses a directly single-quoted literal under a whole-literal const context.
- **tsgo:** internal/pseudochecker/lookup.go:510 typeFromTypeAssertion (from lookup.go:262 typeFromExpression; accessors lookup.go:26 GetTypeOfAccessor), consumed by internal/checker/nodebuilderimpl.go:2181 serializeTypeForDeclaration
- **TSR:** crates/tsr-checker/src/objects.rs:867 check_object_literal_members (property `printed` at :1760)
- **Examples:**
  - `compiler/discriminatedUnionWithIndexSignature`: `{        type: 'text' as const,    }`. want `{ type: 'text'; }`, got `{ type: "text"; }`. typeFromTypeAssertion on `as const` recurses into the StringLiteral and reuses its single-quoted node. TSR's printed arm requires the initializer itself to be a StringLiteral under const context, so an AsExpression falls to member_text_at. Probe ra.ts `a: 'x' as const` matches the oracle.
  - `compiler/objectLiteralComputedNameNoDeclarationError`: `{    BANANA: 'banana' as 'banana',}`. want `{ BANANA: 'banana'; }`, got `{ BANANA: "banana"; }`. `as 'banana'` gives a pseudo-type that is the asserted type node, which is equivalent, so the node is reused with its quotes. Oracle probe `b: 'y' as 'y'` gives the same.
  - `conformance/objectLiteralGettersAndSetters`: `sameType2`. want `{ x: Array<number>; }`, got `{ x: number[]; }`. For an accessor declaration, serializeTypeForDeclaration uses GetTypeOfAccessor, which reuses the getter's `Array<number>` annotation. TSR renders the type; oracle probe `get g(): Array<number>` gives the same.
- **Port plan:** Port the pseudochecker's typeFromExpression/typeFromTypeAssertion/typeFromAccessor (lookup.go) for object-literal members, enough for the as-const literal, `as T`/`<T>` node, and accessor-annotation arms. In check_object_literal_members, or in printing.rs:187 object_literal_text_at, which already certifies checked_declaration, print the pseudo-type node when pseudoTypeEquivalentToType holds. Otherwise keep member_text_at. noUsedBeforeDefinedErrorInTypeContext shows the reuse even where the semantic type is IThing<any>. Note: narrowingNoInfer1's lines are this quote reuse surfacing inside NoInfer<...>; its other lines belong to NOINFER-SUBSTITUTION. Pitfall: printing.rs:273-282 already short-circuits literal slots to 'keep the writer's display'. Move the single source of truth into the pseudo-type path rather than adding a third copy.
- **Finished alone (3):** compiler/discriminatedUnionWithIndexSignature, compiler/noUsedBeforeDefinedErrorInTypeContext, compiler/objectLiteralComputedNameNoDeclarationError
- **Also blocked (other clusters needed) (2):** compiler/narrowingNoInfer1 (+NOINFER-SUBSTITUTION); conformance/objectLiteralGettersAndSetters (+GETTER-RETURN-ANNOTATION-CONTEXT, OBJLIT-ACCESSOR-NONIDENT-NAME)

### 52. `CLASSIFY-PROPERTY-NAME` (V; blocked 5, finished alone 3, lines 8; confidence high)

- **Root cause:** Member names are produced by getPropertyNameNodeForSymbol -> classifyPropertyName. A METHOD named `new` becomes a string literal `"new"`, and a name that is not identifier text, including the parser-recovery empty name, becomes a string literal `""`. TSR spells names from written syntax at each site, with ad-hoc `new` quoting in only some routes; it drops empty names entirely.
- **tsgo:** internal/checker/nodebuilderimpl.go:2384 classifyPropertyName (via :2426 getPropertyNameNodeForSymbol / :2394 createPropertyNameNodeForIdentifierOrLiteral)
- **TSR:** crates/tsr-checker/src/objects.rs:2601 written_property_name (plus per-site name arms: declared.rs:2028 type-literal method signature, objects.rs:1169 object-literal method, signatures.rs:517, spreads.rs:538)
- **Examples:**
  - `compiler/vardecl`: `c`. want `{ "new"?(): any; }`, got `{ new?(): any; }`. `var c: { new? (): any }` is a MethodSignature named new, and classifyPropertyName(isMethod && name=="new") picks a string literal. declared.rs:2029 uses identifier text with no `new` arm; the probe reproduces this.
  - `compiler/emitMethodCalledNew`: `c`. want `{ "new"(x: number): number; }`, got `{ new(x: number): number; }`. The computed `["new"]` method's symbol name is `new` and it is a method, so it is quoted. TSR's `new` arm (objects.rs:1169) covers only the identifier route; probe `{ ["new"](x) }` prints `new(`, while `{ new(x) }` prints `"new"(`.
  - `conformance/FunctionPropertyAssignments2_es6`: `v`. want `{ ""(): Generator<never, void, unknown>; }`, got `{ (): Generator<never, void, unknown>; }`. `{ *() { } }` recovers with an empty-name method symbol. "" is not identifier text, so it prints `""`. written_property_name returns None for an empty identifier (objects.rs:2606), and the member collapses into a call signature.
- **Port plan:** Port classifyPropertyName and getPropertyNameNodeForSymbol (incl. isStringNamed/isSingleQuotedStringNamed and the nameType branch :2455) as one checker helper that takes the property SYMBOL. Route every member-name print through it: the object-literal producer, build_type_literal's property and method arms, spreads.rs, signatures.rs:517 and the anonymous-property renderer. Delete the four ad-hoc `== "new"` checks and written_property_name's duplicate dispatch (its own doc comment already names it 'the copy to delete'). Pitfall: an empty-name member must stay a named method `""`, not a call signature. Also check the EMPTY-NAME-MEMBER family on the same cases, which owns the `> : error` line.
- **Finished alone (3):** compiler/emitMethodCalledNew, compiler/vardecl, conformance/parser645484
- **Also blocked (other clusters needed) (2):** conformance/FunctionPropertyAssignments2_es6 [contextual] (+EMPTY-NAMED-FUNCTION-SYMBOL-TYPE); conformance/FunctionPropertyAssignments6_es6 [contextual] (+EMPTY-NAMED-FUNCTION-SYMBOL-TYPE)

### 53. `ORIGIN-SLICE-GATE` (V; blocked 4, finished alone 3, lines 24; confidence high)

- **Root cause:** getUnionTypeWorker creates a denormalized origin for any union built from named unions (alias/origin) without overlap; TSR build_origin_union's §53 slice gate answers error unless every entry is an enum/alias union or a non-object non-union plain type, so `boolean` entries, object entries (string[], interface) and enum+boolean widenings gap.
- **tsgo:** internal/checker/checker.go:25653 getUnionTypeWorker (addNamedUnions/origin :25705-25728)
- **TSR:** crates/tsr-checker/src/unions.rs:885 build_origin_union (gate :1001-1046)
- **Examples:**
  - `conformance/TypeGuardWithEnumUnion`: `x`. want `string \| string[] \| Color`, got `error`. `Color \| string \| string[]` has object entry string[]; TSR gate returns error (probe `E \| string[]` -> error).
  - `compiler/numberAssignableToEnumInsideUnion`: `z`. want `boolean \| E`, got `any`. `E \| boolean`: boolean entry is a UNION without symbol so fails `plain`; probe `E \| boolean` -> error.
  - `conformance/literalTypes2`: `x3`. want `number \| boolean`, got `error`. let-widening of `E.A \| true \| 123` unions [E, boolean, number]; probe reproduces error (also `cond ? E.A : true` widened).
- **Port plan:** Remove the slice gate and port the origin rule exactly: origin exists iff includes Union (alias or origin inputs) and namedTypesCount+reduced == len(typeSet), entries = reduced non-named members + named unions inserted by CompareTypes; when alias is nil and exactly one named union covers the set, return it. Printing must then render origin entries generally (object entries need parenthesisation like array element types). Pitfall: the gate comment cites site-sensitive alias spellings (temporal, 82 lines) — those need type_to_string_at's per-site origin rendering to handle object entries, measure before/after.
- **Finished alone (3):** compiler/numberAssignableToEnumInsideUnion, compiler/subtypeReductionUnionConstraints, conformance/TypeGuardWithEnumUnion
- **Also blocked (other clusters needed) (1):** conformance/literalTypes2 (+BINDING-PATTERN-CONTEXT-LITERAL, INFERENCE-SUPERTYPE-NAMED-UNION, REMOVE-SUBTYPES-UNDECIDABLE)

### 54. `BINDING-PATTERN-IMPLIED-TYPE` (V; blocked 4, finished alone 3, lines 16; confidence medium)

- **Root cause:** getTypeForVariableLikeDeclaration's final binding-pattern arm returns getTypeFromBindingPattern for ANY declaration with a pattern name and no annotation/initializer (ambient vars, uninitialized vars, rest parameters, parameters of bodyless signatures/function types); TSR only admits it for parameters of non-rest function/method/uncontextual-arrow containers and gaps (or prints any / any[]) everywhere else.
- **tsgo:** internal/checker/checker.go:16790 getTypeForVariableLikeDeclaration (IsBindingPattern arm -> :17904 getTypeFromBindingPattern)
- **TSR:** crates/tsr-checker/src/destructure.rs:396 get_type_for_binding_element_parent (no-source VariableDeclaration -> error at :559; parameter container gate :459) and crates/tsr-checker/src/symbols.rs:4368 get_widened_type_for_variable_like_declaration (gate excludes rest at :4371 and signature containers at :4376)
- **Examples:**
  - `conformance/declarationInAmbientContext`: `a`. want `any`, got `error`. `declare var [a, b];` has no annotation/initializer; tsgo's parent is the implied [any, any] so a is any. TSR's parent function falls to `error` at destructure.rs:559.
  - `compiler/restParameterWithBindingPattern1`: `a`. want `(...{ a, b }: { a: any; b: any; }) => void`, got `(...{ a, b }: any[]) => void`. The pattern arm precedes the rest any[] fallback upstream; TSR's symbols.rs:4371 excludes rest params from the implied-type arm and returns any[] at :4423, so elements a/b index any[] and gap.
  - `compiler/renamingDestructuredPropertyInFunctionType`: `F6`. want `F6`, got `any`. `type F6 = ({ a: string }) => typeof string`: the parameter of a function TYPE gets implied { a: any } so `typeof string` is any. TSR's container gate (destructure.rs:459) excludes FunctionType/ConstructorType/MethodSignature -> element error -> alias error (prints any under strict:false); method2 (0:100) prints its param as any for the same gate in symbols.rs:4376.
- **Port plan:** Port the arm order of checker.go:16652-16794 for pattern-named declarations: after annotation/initializer/catch/for-in/for-of, any declaration whose name is a binding pattern returns getTypeFromBindingPattern(name, includePatternInType=false, reportErrors=true) via binding_patterns.rs binding_pattern_implied_type. Concretely: (1) destructure.rs:559 return the implied type for VariableDeclarations with neither source (ambient or not); (2) drop the `dot_dot_dot_token.is_none()` condition at symbols.rs:4371 so `...{a,b}` gets the pattern type before the any[] fallback; (3) extend the container gate (symbols.rs:4376 and destructure.rs:459, plus signatures.rs parameter_of's twin gate ~4703) to bodyless containers that are never contextually typed: FunctionType, ConstructorType, MethodSignature, CallSignature, ConstructSignature, and declare function / overload declarations. Pitfall: contextually-typed arrows must keep the contextual road (the 78/90 G->W recorded in coAndContraVariantInferences3); and in non-strict noImplicitAny the element for `[a1 = undefined]` must stay `undefined` (want), so check symbols.rs:4128's nullable->any widening does not fire on a pattern-implied element (upstream's getWidenedType keeps a non-widening undefined).
- **Finished alone (3):** compiler/noImplicitAnyDestructuringVarDeclaration, compiler/restParameterWithBindingPattern1, conformance/declarationInAmbientContext
- **Also blocked (other clusters needed) (1):** compiler/renamingDestructuredPropertyInFunctionType (+BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS, CLONE-BINDING-NAME)

### 55. `CHOOSE-OVERLOAD-CONTEXT-SENSITIVE-ARG-RETENTION` (V; blocked 4, finished alone 3, lines 12; confidence medium)

- **Root cause:** Context-sensitive arguments are checked once under the first candidate/pass in chooseOverload (assignContextualParameterTypes sets parameter links once; arrow return types cached) and those types feed later candidates/passes; TSR re-checks per candidate (or skips the subtype pass for generic candidates), losing that retention.
- **tsgo:** internal/checker/checker.go:10349 assignContextualParameterTypes (with :9025 chooseOverload subtype->assignable passes from :8843 resolveCall)
- **TSR:** crates/tsr-checker/src/calls.rs:2480 clean_candidate_prefix_len (excludes generic candidates from subtype pass) + calls.rs:2597 transcribed_generic_set_walk_worker (fresh speculative caches for generic inference)
- **Examples:**
  - `compiler/fixingTypeParametersRepeatedly3`: `bar(derived, d => d.toBase())`. want `Base`, got `Derived`. bar is overloaded so the subtype pass runs first; the arrow's param (Derived) and return Base get fixed then, and the assignable pass infers T from Derived and Base -> Base. Single-sig foo gives Derived in both; TSR treats bar like foo.
  - `conformance/parenthesizedContexualTyping1`: `i`. want `any`, got `number`. fun has two generic overloads; subtype pass fails but the arrows' checked types stay cached, so assignable pass infers T=any (strict off). TSR excludes generic candidates from the subtype pass so arrows are first checked with T=number.
- **Port plan:** Once the faithful chooseOverload loop exists, keep argument checking state across candidates and relations as upstream does: do not evict/re-check context-sensitive arguments between candidates; run the subtype pass over generic candidates too (drop the generic exclusion in clean_candidate_prefix_len); respect assignContextualParameterTypes' set-once semantics and getReturnTypeFromBody caching. Pitfall: this deliberately reproduces upstream's order-dependent results (doYouNeedToChangeYourTargetLibraryES2015 find arrow keeps `=> true` from first-candidate predicate context); TSR's speculative-cache restore in transcribed_generic_set_walk must not wipe these links. Depends on CHOOSE-OVERLOAD-GENERIC-WALK.
- **Finished alone (3):** compiler/fixingTypeParametersRepeatedly1, compiler/fixingTypeParametersRepeatedly3, conformance/parenthesizedContexualTyping1
- **Also blocked (other clusters needed) (1):** compiler/doYouNeedToChangeYourTargetLibraryES2015 (+ARRAY-LITERAL-TUPLE-CONTEXT, CONDITIONAL-INLINE-NODE-INSTANTIATION, INFER-NO-CANDIDATE-GUARD, SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### 56. `CONTEXTUAL-OBJLIT-ELEMENT-INDEX-FALLBACK` (V; blocked 3, finished alone 3, lines 36; confidence high)

- **Root cause:** getContextualTypeForObjectLiteralElement falls back, for any named element (including non-bindable computed names), to the contextual type's applicable index info for getLiteralTypeFromPropertyName(name); TSR returns None as soon as a computed name is not late-bound, so the member gets no contextual type.
- **tsgo:** internal/checker/checker.go:29920 getContextualTypeForObjectLiteralElement (dynamic-name arm :29934, index fallback :29946-29955)
- **TSR:** crates/tsr-checker/src/contextual.rs:2143 contextual_type_for_object_literal_named_element (`self.late_bound_symbol_member_name(name)?.0` at :2163)
- **Examples:**
  - `conformance/computedPropertyNamesContextualType1_ES6`: `y (in `["" + 1]: y => y.length`)`. want `string`, got `any`. `var o: I = { [""+1]: y => y.length }` with I having [s: string]: (x: string) => number: the name type is string, so findApplicableIndexInfo picks the string index and y: string. TSR's `?` at contextual.rs:2163 aborts for the non-literal computed name; probe shows identifier names `a`/`b` under the same I do get `y: string`.
  - `conformance/computedPropertyNamesContextualType2_ES6`: `{    [+"foo"](y) { return y.length; },    [+"bar"]: y => y.length}`. want `{ [x: number]: (y: string) => number; }`, got `error`. `+"foo"` is number, so the number index of the contextual type types both members and the literal gets a number index of `(y: string) => number`; without context TSR's arrow member is error and the literal errors (downstream of the missing contextual type).
- **Port plan:** In contextual_type_for_object_literal_named_element, stop early-returning for computed names: keep the bindable-name lookup (literal/unique-symbol computed names) but when it yields no property (or the name is dynamic), compute the name's literal type (getLiteralTypeFromPropertyName: identifier/string -> string literal, numeric -> number literal, computed -> checked expression type) and mapTypeEx (noReductions) over the apparent contextual type, returning each constituent's findApplicableIndexInfo(getIndexInfosOfStructuredType(t), nameType).valueType. Check whether the existing identifier-name index fallback lives in union_contextual_property_type/get_property_of_type and route computed names through the SAME fallback rather than writing a second one. Pitfalls: the upstream property lookup for usable-as-property-name computed names (:29938) runs before the index fallback; number-typed names select the number index if present else string (findApplicableIndexInfo), and this must reach both the method (getContextualTypeForObjectLiteralMethod) and property-assignment paths.
- **Finished alone (3):** conformance/computedPropertyNamesContextualType1_ES6, conformance/computedPropertyNamesContextualType2_ES6, conformance/computedPropertyNamesContextualType3_ES6

### 57. `CHOOSE-OVERLOAD-WRITTEN-TYPE-ARGUMENTS` (V; blocked 3, finished alone 3, lines 14; confidence high)

- **Root cause:** chooseOverload's hasCorrectTypeArgumentArity filter + instantiation of the candidate with explicit type arguments (getTypeArgumentsFromNodes) is missing on the overloaded call and tagged-template roads; TSR's multi-candidate road declines written type arguments.
- **tsgo:** internal/checker/checker.go:9214 hasCorrectTypeArgumentArity (used in :9025 chooseOverload)
- **TSR:** crates/tsr-checker/src/calls.rs:1720 choose_overload (has_type_arguments excluded) + calls.rs:1162 check_tagged_template_expression + contextual.rs:2446 contextual_type_for_argument_resolving
- **Examples:**
  - `compiler/callbacksDontShareTypes`: `_.map<number, string>(c2, (x) => { return x.toFixed() })`. want `Collection<string>`, got `any`. Two map overloads; written <number,string> selects the 2-type-param candidate and contextually types x:number. Oracle probe map<number,string>(c2,x=>x.toFixed()) tsgo Col<string>, TSR error; single-signature twin right.
  - `compiler/tupleTypeInference`: `[$q.when<string>(), $q.when<number>()]`. want `[IPromise<string>, IPromise<number>]`, got `(IPromise<string> \| IPromise<number>)[]`. $q.all<string,number> overloads differ by type-param count; arity filter picks the [T,U] tuple candidate giving tuple context. Probe tup<string,number>(['a',1]) tsgo [string,number], TSR error/array.
  - `compiler/genericTemplateOverloadResolution`: `fooFn<number>```. want `Promise<number>`, got `Promise<{}>`. Tagged-template road filters only by argument arity, so the non-generic overload wins; upstream drops it via hasCorrectTypeArgumentArity.
- **Port plan:** In the shared chooseOverload port, filter candidates with hasCorrectTypeArgumentArity when typeArguments are written, and for generic candidates instantiate with getTypeArgumentsFromNodes (fill defaults via fillMissingTypeArguments) before checking applicability and assigning contextual types. Route tagged templates through the same chooser instead of check_tagged_template_expression's own survivor filter, and remove contextual_type_for_argument_resolving's by-arity agreement slice once the resolved signature drives argument context. Pitfall: argument contextual types must come from the chosen instantiated candidate (callbacks, array-literal tuple context).
- **Finished alone (3):** compiler/callbacksDontShareTypes, compiler/genericTemplateOverloadResolution, compiler/tupleTypeInference

### 58. `LATE-BIND-INDEX-SIGNATURE-CLASS-INTERFACE` (V; blocked 3, finished alone 3, lines 9; confidence high)

- **Root cause:** Class/interface members whose computed name is an entity-name expression of non-literal string/number/symbol type are late-bound into the __index symbol (getResolvedMembersOrExportsOfSymbol -> lateBindIndexSignature) and getIndexInfosOfIndexSymbol synthesizes a [x: K] index info over them plus sibling members; TSR's index_infos_of_symbol only reads written IndexSignatureDeclarations.
- **tsgo:** internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (hasLateBindableIndexSignature arm :19662; aggregation :19700-19716 via getObjectLiteralIndexInfo :19721), fed by checker.go:15953 getResolvedMembersOrExportsOfSymbol arm -> :16068 lateBindIndexSignature
- **TSR:** crates/tsr-checker/src/index_signatures.rs:375 index_infos_of_symbol
- **Examples:**
  - `compiler/classNonUniqueSymbolMethodHasSymbolIndexer`: `e1`. want `() => number`, got `error`. `class A { [a]() {...} }` with `a: symbol` (entity name, non-unique) is a late-bindable index signature, so A gets [x: symbol]: () => number and `A[typeof a]` resolves. TSR's index_infos_of_symbol (ClassDeclaration arm, :431) collects only ClassElement::IndexSignatureDeclaration, so A has no index info and the indexed access errors; ad-hoc oracle confirms (`e1 : () => number` vs TSR error).
  - `compiler/declarationEmitComputedNameWithQuestionToken`: `(new WithData())["ahahahaahah"]`. want `(() => string) \| undefined`, got `error`. `[dataSomething]?()` where dataSomething: `data-${string}` (assignable to string) yields a [x: string] index info valued by the optional method's type; TSR builds no index info for the class so the element access errors. Oracle on `class B { [k]?() {...} }` with k: string: tsgo `(() => string) \| undefined`, TSR error; the same gap appears for interfaces (`interface I { [k]: number; y: string }`, `
- **Port plan:** Add the hasLateBindableIndexSignature arm to the class/interface walk in index_infos_of_symbol: for each member (instance or static per `static_side`) whose name is a ComputedPropertyName with an entity-name expression (isLateBindableAST) whose checked type is assignable to string|number|symbol but NOT usable as a property name (not string/number literal or unique symbol), skip if an explicit index info for that key exists, else classify key number->symbol->string (number asked first, so any/error keys are number) and track readonly. Then append getObjectLiteralIndexInfo(readonly, propertySymbols, key) where propertySymbols = the computed members plus ALL sibling members of the same side (string key: every non-symbol-named sibling; number: numeric-named; symbol: symbol-named), value = union of their types. Pitfalls: static side siblings include the class `prototype` export (declarationEmitSimpleComputedNames1 `Holder["some"+"thing"]` wants `Holder | ...`); merged same-named late-bound overloads (`static [staticField]()` twice) contribute one symbol each declaration via the index symbol's declarations; reuse a shared helper with the type-literal cluster rather than a second copy; base-type index inheritance in the same function must see these infos first (own wins by key).
- **Finished alone (3):** compiler/classNonUniqueSymbolMethodHasSymbolIndexer, compiler/declarationEmitComputedNameWithQuestionToken, compiler/declarationEmitSimpleComputedNames1

### 59. `OBJLIT-PROPERTIES-TABLE-ESCAPED-NAME-KEY` (V; blocked 3, finished alone 3, lines 8; confidence high)

- **Root cause:** checkObjectLiteral keys propertiesTable by the member symbol's escaped name (binder getPropertyNameForPropertyNameNode merges `26`, `0b11010`, `"26"`; late-bound computed names use getPropertyNameFromType, so `"1"` and `[+1]` collide), later entries replacing earlier; TSR's printed member list is upserted by PRINTED spelling, so quoted and numeric spellings of one name survive as two members.
- **tsgo:** internal/checker/checker.go:13144 checkObjectLiteral (propertiesTable[member.Name] = member at :13331)
- **TSR:** crates/tsr-checker/src/objects.rs:2445 upsert_member (called from check_object_literal at :1839 with the printed `name`)
- **Examples:**
  - `conformance/binaryIntegerLiteralError`: `obj1`. want `{ 26: string; }`, got `{ 26: string; "26": string; }`. `{ 0b11010: "hi", 26: "Hello", "26": "world" }`: all three names are "26", one binder symbol, one propertiesTable entry. TSR's typed_properties already key by semantic name (:1797-1834) but upsert_member compares printed names `26` vs `"26"`, so the rendered type has two members. octalIntegerLiteralError is the same shape (not independent).
  - `compiler/duplicateObjectLiteralProperty_computedName1`: `t3`. want `{ 1: number; }`, got `{ "1": number; 1: number; }`. `{ "1": 1, [+1]: 0 }`: the computed `+1` late-binds via getPropertyNameFromType to "1", replacing the string-literal entry; TSR prints `"1"` and `1` as distinct printed keys.
- **Port plan:** Key the rendered `members` list by the same semantic (escaped) name typed_properties already uses (`semantic_name`), not the printed spelling: give Member::Property/Method an optional key or upsert by index into a parallel key vector, and replace in place (first position, last value). The printed name must then come from the winning symbol's declarations as the node builder does: binder-merged literal names print from the merged symbol (first declaration numeric -> `26`, even though the last is `"26"`), while a later computed late-bound entry replaces the symbol and prints its own form (`[-1]` in t6, `1` in t3). Pitfalls: method/accessor arms (:1074, :1143, :1174, :1255, :1409) must use the same key; index-info rows and signatures stay unkeyed.
- **Finished alone (3):** compiler/duplicateObjectLiteralProperty_computedName1, conformance/binaryIntegerLiteralError, conformance/octalIntegerLiteralError

### 60. `SUPER-CALL-CONSTRUCTOR-ONLY` (V; blocked 3, finished alone 3, lines 6; confidence high)

- **Root cause:** checkSuperExpression's isLegalUsageOfSuperExpression requires a super CALL's container to be a Constructor; TSR accepts super() in any class member (property initializer, method, accessor, static property) and answers typeof Base instead of errorType.
- **tsgo:** internal/checker/checker.go:7866 checkSuperExpression.isLegalUsageOfSuperExpression (isCallExpression arm :7867-7870; func :7854)
- **TSR:** crates/tsr-checker/src/expressions.rs:2007 check_super_expression
- **Examples:**
  - `compiler/superCallOutsideConstructor`: `super`. want `any`, got `typeof C`. `x = super()` in a property initializer: container is PropertyDeclaration, call arm requires IsConstructorDeclaration -> errorType. TSR's member walk (:2067) accepts PropertyDeclaration regardless of is_call; probe sup.ts shows `super : typeof B` for x=super() and m(){super()}.
  - `conformance/typeOfThisInStaticMembers6`: `super`. want `any`, got `typeof C`. `static c = super()`: same call arm, static property container is illegal for a call.
- **Port plan:** After the container walk in check_super_expression, port isLegalUsageOfSuperExpression: if is_call, require the found member kind to be Constructor else return error; for non-call keep the existing static/instance kind table (add ClassStaticBlockDeclaration/PropertySignature per :7879-7882). Record the member kind in the walk instead of only is_static. Pitfall: the arrow arm at :2062 already returns any for is_call — keep it consistent (errorType).
- **Finished alone (3):** compiler/superCallOutsideConstructor, conformance/errorSuperCalls, conformance/typeOfThisInStaticMembers6

### 61. `TYPELIT-DUPLICATE-PROPERTY-SYMBOL-MERGE` (V; blocked 3, finished alone 3, lines 5; confidence high)

- **Root cause:** The binder's declareSymbol merges same-named property signatures of a type literal into one member symbol, so resolveAnonymousTypeMembers yields one property; TSR's build_type_literal renders one Member::Property per declaration with no name dedupe.
- **tsgo:** internal/binder/binder.go:152 declareSymbolEx (same-name merge) -> internal/checker/checker.go:16124 getMembersOfSymbol
- **TSR:** crates/tsr-checker/src/declared.rs:1935 build_type_literal (property push at :2421-2435)
- **Examples:**
  - `compiler/propertySignatures`: `foo1`. want `{ a: string; }`, got `{ a: string; a: string; }`. `declare var foo1: { a:string; a: string; }` binds one symbol `a`; TSR pushes both declarations into `properties` and `typed_properties`.
  - `conformance/duplicatePropertiesInTypeAssertions01`: `<{a: number; a: number}>{}`. want `{ a: number; }`, got `{ a: number; a: number; }`. Same duplicate-signature literal reached through a type assertion; 02 is the `as` twin (not independent).
- **Port plan:** In build_type_literal, key property/method members by their binder symbol (or semantic key from type_literal_member_key) and emit one member per symbol at its first declaration's position; the symbol's type is getTypeOfSymbol of the merged symbol (for duplicate property signatures upstream uses the first declaration's annotation — check getTypeOfVariableOrParameterOrProperty uses valueDeclaration). typed_properties must be deduped the same way. Pitfall: methods already merge into overload sets via `existing` (:2117) — keep that; accessor arm keeps its own pairing.
- **Finished alone (3):** compiler/propertySignatures, conformance/duplicatePropertiesInTypeAssertions01, conformance/duplicatePropertiesInTypeAssertions02

### 62. `INFER-NO-CANDIDATE-GUARD` (V; blocked 5, finished alone 2, lines 26; confidence high)

- **Root cause:** getInferredType's no-candidate arm (default type -> unknown, then constraint) is blocked by TSR's 'structural_source_supplied' refusal: when an argument mentions the type parameter's position but inference collected no candidate, the call declines to error.
- **tsgo:** internal/checker/inference.go:1317 getInferredType
- **TSR:** crates/tsr-checker/src/inference.rs:560 check_generic_call_worker (no-candidate decline L1371-1416)
- **Examples:**
  - `compiler/implicitIndexSignatures`: `getNumberIndexValue(E2)`. want `unknown`, got `any`. String enum has no numeric-named props, so T gets no candidate -> unknown upstream; TSR guard declines. Oracle probes q<T>(x:{a?:T}) q({}) and s<T>(fn:(o:T)=>void) s(()=>{}) tsgo unknown, TSR error; no-argument u<T>() gives unknown in TSR (guard not hit).
  - `compiler/acceptSymbolAsWeakType`: `new FinalizationRegistry(() => {})`. want `FinalizationRegistry<unknown>`, got `error`. Callback with fewer params contributes no candidate for T; same guard. Oracle c4<R>(t:new()=>R) c4(()=>{}) tsgo unknown, TSR error.
  - `compiler/contextualParamTypeVsNestedReturnTypeInference4`: `effectGen(function* () {...})`. want `Effect<{...}, unknown, unknown>`, got `error`. Generator<never,...> source yields no candidates for E,R (never source); oracle k4<A>(f:{x:A}) k4(nv as never): tsgo unknown, TSR error.
- **Port plan:** Delete the structural_source_supplied decline in check_generic_call_worker so candidate-free parameters fall through to resolve_inference_with_constraints' getInferredType default/constraint path (already correct for no-arg calls). Pitfall: the guard exists because TSR's inferFromTypes collector may miss candidates upstream finds; removing it turns those gaps into WRONG lines, so measure and fix collector misses (e.g. reverse-mapped templates, see contravariantOnlyInferenceFromAnnotatedFunction) rather than re-adding the guard. Keep InferenceFlagsAnyDefault for JS files.
- **Finished alone (2):** compiler/declarationEmitOverloadedPrivateInference, compiler/implicitIndexSignatures
- **Also blocked (other clusters needed) (3):** compiler/acceptSymbolAsWeakType (+ARG-CONTEXT-RESOLVED-SIG); compiler/contextualParamTypeVsNestedReturnTypeInference4 (+MERGED-FUNCTION-INTERFACE, unaligned); compiler/doYouNeedToChangeYourTargetLibraryES2015 (+ARRAY-LITERAL-TUPLE-CONTEXT, CHOOSE-OVERLOAD-CONTEXT-SENSITIVE-ARG-RETENTION, CONDITIONAL-INLINE-NODE-INSTANTIATION, SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### 63. `CHOOSE-OVERLOAD-GENERIC-WALK` (V; blocked 5, finished alone 2, lines 22; confidence medium)

- **Root cause:** chooseOverload's per-candidate loop for sets containing generic candidates (infer type args with this candidate, inferential arg check, applicability; fall through on failure) is replaced by transcribed_generic_set_walk which declines on undecidable inference/relations or on any written type args, so calls error or pick a later candidate.
- **tsgo:** internal/checker/checker.go:9025 chooseOverload
- **TSR:** crates/tsr-checker/src/calls.rs:2546 transcribed_generic_set_walk / :2597 transcribed_generic_set_walk_worker (gated at calls.rs:1734 choose_overload)
- **Examples:**
  - `compiler/ipromise2`: `p.then(function (s) { return 34; })`. want `Windows.Foundation.IPromise<number>`, got `any`. then is overloaded generic (IPromise<U> / U callbacks); first candidate must be rejected and the second inferred. Probe k<U>(x:{then():U}):U[]; k<U>(x:U):U; k(34): tsgo number, TSR error (oracle); non-generic twin k2 works.
  - `conformance/intersectionTypeInference3`: `Array.from(a)`. want `A[]`, got `any`. Array.from overloads ArrayLike<T> then Iterable<T>; first generic candidate not applicable for Set. Probe sel<T>(ArrayLike<T>)/sel<T>(Iterable<T>) sel(s): tsgo string[], TSR error; non-generic h2 picks correctly.
  - `compiler/jsxGenericComponentWithSpreadingResultOfGenericFunction`: `omit(['bar'], otherProps)`. want `Omit<..., "bar">`, got `Omit<..., string>`. First generic overload must infer K='bar' with inferential (literal-preserving) arg check; TSR probe om(['bar'],o) picks the second overload's answer.
- **Port plan:** Port chooseOverload's loop body faithfully for generic candidates: per candidate create inference context, inferTypeArguments with CheckModeInferential (|SkipContextSensitive first pass), getSignatureInstantiation, getSignatureApplicabilityError under the pass relation, and the second inference round for context-sensitive args; run under subtype then assignable relation from resolveCall. Replace transcribed_generic_set_walk's declines (unknown relation, undecidable inference) and the agreement/ladder heuristics in choose_overload. Pitfalls: speculative caches of nested generic calls and context-sensitive arrows must be reset per candidate (nonInferrableTypePropagation2 fixes n=unknown under the failing first candidate); relater Unknown for Set vs ArrayLike must become a decision.
- **Finished alone (2):** compiler/jsxGenericComponentWithSpreadingResultOfGenericFunction, compiler/nonInferrableTypePropagation2
- **Also blocked (other clusters needed) (3):** compiler/ipromise2 (+PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED, RENAMED-TYPE-PARAM-KEEPS-WRITTEN-NODE); compiler/twiceNestedKeyofIndexInference (+SHADOWED-TYPEPARAM-RENAME); conformance/intersectionTypeInference3 (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS)

### 64. `IMPORT-TYPE-NODE-TYPEOF` (V; blocked 5, finished alone 2, lines 19; confidence high)

- **Root cause:** getTypeFromImportTypeNode's value-meaning arm (`typeof import("m")[.q]`, qualifier resolved through property-of-type, plus instantiation-expression type arguments via resolveImportSymbolType) is not ported; TSR returns error (unknown in JSDoc) for every is_type_of import type.
- **tsgo:** internal/checker/checker.go:24575 getTypeFromImportTypeNode (IsTypeOf: :24608-24609 property lookup, :24660-24662 getInstantiationExpressionType)
- **TSR:** crates/tsr-checker/src/declared.rs:4600 get_type_from_import_type_node (`if node.is_type_of` early error at :4602)
- **Examples:**
  - `compiler/moduleResolutionWithRequireAndImport`: `a`. want `typeof import("./other")`, got `error`. `const a: typeof import('./other')` -> value meaning, no qualifier -> getTypeOfSymbol(module). TSR declines is_type_of (probe p7 `b` error vs oracle typeof import("./id")).
  - `compiler/importTypeTypeofClassStaticLookup`: `foo`. want `() => void`, got `error`. `typeof import("./a").A.foo` walks A then `foo` as a property of typeof A (getPropertyOfType), not exports. TSR declines (probe p7 `c` error vs oracle () => void).
- **Port plan:** Add the IsTypeOf arm: walk qualifier segments with getPropertyOfType(getTypeOfSymbol(current)) for value meaning (exports for intermediate namespace segments), then type = getInstantiationExpressionType(getTypeOfSymbol(symbol), node) so `typeof import(...).f<A,B>` instantiates (typeofImportInstantiationExpression T2). Unqualified: getTypeOfSymbol(module) — the module object must print `typeof import("./other")` / `typeof import("deps/BaseClass")` (JSDoc in amdLike) via the module-specifier spelling in type_to_string_at_worker; JS targets and `.default` qualifiers (jsDeclarationEmitDoesNotRenameImport) go through the same walk. Pitfall: the doc comment cites the `bd tsr-e2u` naming wall — the printed form for a module object reached through no alias must be the import specifier, not the file path.
- **Finished alone (2):** compiler/importTypeTypeofClassStaticLookup, compiler/moduleResolutionWithRequireAndImport
- **Also blocked (other clusters needed) (3):** compiler/amdLikeInputDeclarationEmit (+JS-LITERAL-PROPERTY-ANY, RETURN-LITERAL-OWN-SIG-CONTEXT); compiler/jsDeclarationEmitDoesNotRenameImport (+CLASS-GET-BASE-TYPES); compiler/typeofImportInstantiationExpression (+GET-TYPE-OF-NODE-NON-EXPRESSION-ERRORTYPE, LOGICAL-OR-COALESCE-GENERIC-GATE)

### 65. `EXTERNAL-MODULE-MEMBER-EXPORT-EQUALS` (V; blocked 4, finished alone 2, lines 25; confidence medium)

- **Root cause:** getExternalModuleMember on an export= module takes symbolFromVariable = getPropertyOfType(typeof target, name) and combines it with symbolFromModule; TSR declines every value member whose type is not a site-independent scalar, or that is an ALIAS, and then also drops the supplemental symbol (symbols.rs:1834).
- **tsgo:** internal/checker/checker.go:14667 getExternalModuleMember (export= branch 14698-14732)
- **TSR:** crates/tsr-checker/src/symbols.rs:1737 get_external_module_member (gate 1795-1818, 1834)
- **Examples:**
  - `compiler/exportAssignedNamespaceIsVisibleInDeclarationEmit`: `f`. want `() => import("./thing").Bar`, got `any`. `export = Foo` (a namespace with function f). The value f is found but its function type is not 'site independent', so it is declined; supplemental Foo.exports.f exists, so line 1834 returns None. Probe p5: f any, n (number) correct.
  - `compiler/exportEqualsOfModule`: `Request`. want `typeof Request`, got `any`. popsicle `export = alias` (an import= of a module re-exporting Request). The property found is an ALIAS and is declined at 1797; no supplemental exists, so the result is None.
- **Port plan:** Remove the site-independence and ALIAS gates. Return symbolFromVariable (resolve it if it is an export alias, per the 14703 comment), combined with symbolFromModule via combineValueAndTypeSymbols as upstream does, including the synthetic-symbol case. The gates were defensive against the per-site naming wall (§158/tsr-4jk): measure the import("./thing").Bar spelling lines after the change, since those need the per-site naming lane to print right.
- **Finished alone (2):** compiler/exportAssignedNamespaceIsVisibleInDeclarationEmit, compiler/exportEqualsOfModule
- **Also blocked (other clusters needed) (2):** compiler/contextuallyTypedJsxAttribute2 (+SYNTHETIC-DEFAULT-IMPORT-TARGET); compiler/importDeclFromTypeNodeInJsSource (+SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER)

### 66. `CONTEXTUAL-RETURN-GENERATOR-FILTER` (V; blocked 4, finished alone 2, lines 20; confidence high)

- **Root cause:** getContextualReturnType's generator arm (filter a union contextual return type to constituents a generator can be assigned to) and getContextualIterationType (iteration types of that filtered type) are unported; TSR reads contextual iteration slots only off a single Generator-family type reference (contextual_generator_iteration_type / contextual_type_for_yield_operand), so union contextual return types give no next/yield context.
- **tsgo:** internal/checker/checker.go:29665 getContextualReturnType (generator filter :29678-29682) ; :29656 getContextualIterationType ; :29719 getContextualTypeForYieldOperand (non-star union filter)
- **TSR:** crates/tsr-checker/src/contextual.rs:1571 contextual_generator_iteration_type (callers: crates/tsr-checker/src/expressions.rs:3656-3663, crates/tsr-checker/src/signatures.rs:2554-2568 async gate and 3097-3103 next fallback); crates/tsr-checker/src/contextual.rs:1769 contextual_type_for_yield_operand
- **Examples:**
  - `compiler/contextualTypeOnYield1`: `num  (f: () => (number \| Generator<(arg: number) => void, any, void>) = function*() { yield (num) => ... })`. want `number`, got `any`. getContextualReturnType filters `number \| Generator<...>` to the Generator constituent, so the yield operand is contextually (arg: number) => void. TSR's contextual_type_for_yield_operand requires the contextual return to be a single reference and returns None, leaving num any (and the yield/function error).
  - `compiler/typeOfYieldWithUnionInContextualReturnType`: `yield "What is your name?"  (looserSyncFactory: SyncFactory \| AsyncFactory)`. want `string`, got `error`. The union contextual signature returns Generator<string,string,string> \| AsyncGenerator<...>; the generator filter keeps the sync one, next = string. TSR's check_yield_expression contextualised arm calls contextual_generator_iteration_type on the union and answers error; for the async sibling the same failure trips the async gate in return_type_from_body (0:40).
  - `compiler/contextuallyTypeGeneratorReturnTypeFromUnion`: `yield ''  (test1: () => (Generator<string, string, string[]> \| string))`. want `string[]`, got `error`. Filtering drops `string`, next = string[]. Probe t4 shows the same with an anonymous union, while plain `() => Generator<...>` (t3) is right in TSR.
- **Port plan:** Port getContextualReturnType in full (annotation; contextual signature return when not resolving, filtered for generators by AnyOrUnknown|Void|InstantiableNonPrimitive or checkGeneratorInstantiationAssignabilityToReturnType, for async by getAwaitedTypeOfPromise; then the IIFE arm, which G18 owns as CONTEXTUAL-RETURN-IIFE - port the function once, both families close together) and getContextualIterationType = getIterationTypeOfGeneratorFunctionReturnType(kind, contextualReturnType) from GENERATOR-ANNOTATION-ITERATION-TYPES. Replace contextual_generator_iteration_type with it at all three callers (check_yield_expression's contextualised arm, the async gate in return_type_from_body which should then disappear, and the next-slot fallback), and rewrite contextual_type_for_yield_operand to getContextualTypeForYieldOperand (non-star: filter union by getIterationTypeOfGeneratorFunctionReturnType(Return) != nil, then Yield slot; star: build Generator/AsyncGenerator union from iteration types). Pitfall: getWidenedLiteralLikeTypeForContextualIterationTypeIfNeeded in getReturnTypeFromBody also reads the contextual return type, which keeps literal 0 in generatorYieldContextualType's AsyncGenerator<0, 0, 1>.
- **Finished alone (2):** compiler/contextualTypeOnYield1, compiler/typeOfYieldWithUnionInContextualReturnType
- **Also blocked (other clusters needed) (2):** compiler/contextuallyTypeGeneratorReturnTypeFromUnion (+YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE); conformance/generatorYieldContextualType (+APPEND-LOCAL-TYPE-PARAMETERS, CONDITIONAL-BRANCH-NAMED-UNION, GENERATOR-ANNOTATION-ITERATION-TYPES, INDEXED-ACCESS-GENERIC-DEFER, ORIGIN-ENTRY-ORDER, REMOVE-SUBTYPES-UNDECIDABLE, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE, unaligned)

### 67. `RESOLVE-ALIAS-INDIRECTION` (V; blocked 4, finished alone 2, lines 18; confidence high)

- **Root cause:** resolveAlias applies resolveIndirectionAlias when the immediate target is itself a non-local pure alias, so alias targets are final; TSR's resolve_alias is one hop, and the type-reference road and the writer's type-only arm then see an ExportSpecifier ALIAS with no TYPE meaning.
- **tsgo:** internal/checker/checker.go:16266 resolveAlias (16280 IsNonLocalAlias → 16293 resolveIndirectionAlias); 24094 getDeclaredTypeOfAlias
- **TSR:** crates/tsr-checker/src/symbols.rs:943 resolve_alias (consumers: declared.rs:1565 alias road TYPE test)
- **Examples:**
  - `conformance/enums`: `flags`. want `SymbolFlags`, got `any`. a.ts `const enum SymbolFlags{}; export { SymbolFlags }`, b.ts imports it. resolve_alias stops at the export-specifier alias, and declared.rs:1581's TYPE test fails, giving error. Probe p3: `class C{}; export { C }` then `let c: C` gives error.
  - `compiler/decoratorMetadataTypeOnlyExport`: `par`. want `Foo`, got `error`. `class Foo {}; export type { Foo }` then `par: Foo` in another file: same one-hop stop at the local export specifier.
  - `compiler/noCrashOnImportShadowing`: `x`. want `B`, got `any`. index imports B, which a.ts re-exports via `export { B }`, where B is import*-alias merged with an interface. Upstream's indirection resolves the specifier to a.B (not a pure alias, so it stops there) and declares interface B; TSR stops at the specifier. Probe p11 reproduces this with a plain interface.
- **Port plan:** Port resolveIndirectionAlias: after computing the immediate target in resolve_alias, if the target's flags & (ALIAS|VALUE|TYPE|NAMESPACE) == ALIAS (IsNonLocalAlias), recurse into resolve_alias(target) and merge the result. Restore the AliasTarget resolution frame that the doc comment at 907-925 says must come back (a_re_export_cycle_between_two_files_terminates), and ideally memoise like aliasSymbolLinks.aliasTarget. Then remove the compensations that walk the chain one level up (get_symbol_flags chain walk stays, as it is upstream's seenSymbols). Pitfalls: the writer's import-type specifier line (enums 1:0 `SyntaxKind : SyntaxKind`) uses the resolved target's declared type; and renamed/naming gates in declared.rs (§491/§158) still apply.
- **Finished alone (2):** compiler/decoratorMetadataTypeOnlyExport, conformance/enums
- **Also blocked (other clusters needed) (2):** compiler/declarationsForIndirectTypeAliasReference (+DEFERRED-TYPE-REFERENCE-ALIAS); compiler/noCrashOnImportShadowing (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS, TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL)

### 68. `YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE` (V; blocked 4, finished alone 2, lines 6; confidence high)

- **Root cause:** checkAndAggregateYieldOperandTypes adds getContextualType(yield) to the generator's NEXT aggregate for every non-star yield (nil adds nothing; the contextual-iteration fallback then fills the slot); TSR instead keeps a parent-kind decline list and refuses the whole signature when the yield sits in a call argument, decorator, or (in a contextually typed generator) any variable/return position.
- **tsgo:** internal/checker/checker.go:20322 checkAndAggregateYieldOperandTypes (nextType = getContextualType(yieldExpr) :20338-20340) via :20126 getReturnTypeFromBody
- **TSR:** crates/tsr-checker/src/signatures.rs:2497 return_type_from_body (decline predicate 2728-2868, recorded_next arms 2869-2971: `if contextual && !recorded_next { return None }`)
- **Examples:**
  - `conformance/generatorTypeCheck39`: `g  (function* g() { @decorator(yield 0) class C { x = yield 0 } })`. want `() => Generator<number, void, any>`, got `any (error)`. The yield is argument 0 of decorator(x: any), so getContextualType(yield) = any lands in the next aggregate. TSR lists CallExpression as a contextual parent without computing the type and returns None for the signature (probe t5 `take(yield 0)` with x: number: tsgo Generator<number, void, number>, TSR error).
  - `conformance/generatorYieldContextualType`: `function* () { const a = yield 0; return 0; }  (argument of f1<0,0,1>)`. want `() => Generator<0, 0, 1>`, got `error`. The unannotated `const a` gives the yield no contextual type, so nextTypes stays empty and the slot falls back to getContextualIterationType(Next) = 1. TSR's expression-side decline list includes VariableDeclaration and refuses (probe t1 with plain Generator context: same error).
- **Port plan:** Delete the decline predicate and the three hand-written recorded_next arms (variable/property annotation, assertion, `x = yield`) and call the general get_contextual_type(yield_id) for each non-star yield, appending a Some result to next_types (yield* next comes from the iteration engine in YIELD-STAR clusters); keep the fallback getContextualIterationType(Next) orElse unknown when empty. Pitfall: TSR's get_contextual_type returns None both for 'no contextual type' and for unported arms - audit the arms these positions hit (call argument through contextual_type_for_argument, decorator getContextualTypeForDecorator, binary operand, assertion) before deleting the list, or unported arms become WRONG next slots instead of gaps. contextuallyTypeGeneratorReturnTypeFromUnion 0:11 and generatorYieldContextualType 0:13 are async generators under a union contextual return and also need CONTEXTUAL-RETURN-GENERATOR-FILTER (the async gate at signatures.rs:2554).
- **Finished alone (2):** conformance/generatorTypeCheck39, conformance/generatorTypeCheck61
- **Also blocked (other clusters needed) (2):** compiler/contextuallyTypeGeneratorReturnTypeFromUnion (+CONTEXTUAL-RETURN-GENERATOR-FILTER); conformance/generatorYieldContextualType (+APPEND-LOCAL-TYPE-PARAMETERS, CONDITIONAL-BRANCH-NAMED-UNION, CONTEXTUAL-RETURN-GENERATOR-FILTER, GENERATOR-ANNOTATION-ITERATION-TYPES, INDEXED-ACCESS-GENERIC-DEFER, ORIGIN-ENTRY-ORDER, REMOVE-SUBTYPES-UNDECIDABLE, unaligned)

### 69. `INTERFACE-BASE-FROM-TYPE-NODE` (V; blocked 3, finished alone 2, lines 30; confidence high)

- **Root cause:** resolveBaseTypesOfInterface takes getTypeFromTypeNode of each heritage entry (alias resolved and instantiated: alias-to-object-literal, alias-to-intersection, Record<..> mapped type) and accepts any isValidBaseType object/intersection; TSR derives bases by symbol (base_symbols_of_ex) and drops non-interface-reference bases, so inherited members are missing.
- **tsgo:** internal/checker/checker.go:19498 resolveBaseTypesOfInterface (+ :19537 isValidBaseType)
- **TSR:** crates/tsr-checker/src/members.rs:3182 base_symbols_of_ex / base_symbol_of_heritage_entry :3211
- **Examples:**
  - `conformance/genericTypeAliases`: `p.a`. want `number`, got `any`. `interface TaggedPair<T> extends Pair<T>` where Pair is an alias; upstream base = instantiated alias type. Probe base.ts `interface P<T> extends AB<T,T>` -> p.a error; even non-generic `type QA = Q; interface Q2 extends QA` -> q.a error.
  - `compiler/spreadOfObjectLiteralAssignableToIndexSignature`: `recordOfRecords.propA`. want `RecordOfRecords`, got `error`. `extends Record<keyof any, ..>`: Record is an alias to a mapped type; base must be the resolved mapped type with index signature. Probe `interface R extends Record<string, number>` r.x -> error.
- **Port plan:** Make interface base resolution type-based: for each extends entry compute get_type_from_type_node (alias-instantiated), filter with isValidBaseType (object non-generic-mapped, intersection of such, any), and feed those types to resolveObjectTypeMembers's base member/index/signature merge with this-type instantiation (intersectionThisTypes needs the base's `this` bound to Thing5). Replace symbol-based base lookups for interfaces (base_symbols_of_ex callers) or add a type-based path. Pitfall: mapped base (Record) needs resolved mapped members incl. index signatures; keep class bases on their own path.
- **Finished alone (2):** compiler/spreadOfObjectLiteralAssignableToIndexSignature, conformance/intersectionThisTypes
- **Also blocked (other clusters needed) (1):** conformance/genericTypeAliases (+TYPE-ALIAS-ACCESSIBILITY-GATE)

### 70. `JSDOC-ON-FUNCTION-EXPRESSION-HOSTING` (V; blocked 3, finished alone 2, lines 18; confidence high)

- **Root cause:** The parser attaches leading JSDoc to arrow functions/function expressions (parseParenthesizedArrowFunctionExpression/parseSimpleArrowFunctionExpression/parseFunctionExpression carry jsdoc and reparseTags runs on them), so a @param/@returns directly before an expression-position function types it; TSR's parser never calls parse_leading_jsdoc for these expressions, so jsdoc_entries has no host.
- **tsgo:** internal/parser/parser.go:4341 parseParenthesizedArrowFunctionExpression (+ :4541 parseSimpleArrowFunctionExpression, :5677 parseFunctionExpression) -> reparser.go:54 reparseTags / :475 JSDocParameterTag arm
- **TSR:** crates/tsr-parser/src/expression.rs:1647 try_parse_arrow_function (and :2217 parse_function_expression)
- **Examples:**
  - `compiler/arrowFunctionJSDocAnnotation`: `param => param`. want `(param: number) => number \| undefined`, got `(param: any) => any`. `/** @param {number} param @returns {number=} */ param => param` as a call argument; tsgo hosts the comment on the ArrowFunction. Probe: `/** @param {number} p */ p => p` in call arg, in `const y = /**/ q=>q` and in parens all give any, while the same doc on the VariableStatement works.
  - `compiler/contravariantOnlyInferenceWithAnnotatedOptionalParameterJs`: `(pose) => true`. want `(pose?: number) => true`, got `(pose: any) => true`. `@param {number} [pose]` on the arrow argument: reparser adds type + question token; TSR never sees the doc so pose is any and filter infers any.
- **Port plan:** Port the parser's JSDoc capture for expression-position functions: call parse_leading_jsdoc at the start of arrow (both simple and parenthesized forms) and function-expression parsing and attach_jsdoc to the resulting node (upstream: hasPrecedingJSDocComment captured in parseAssignmentExpressionOrHigher/arrow paths). jsdoc_parameter_annotation and signatures.rs:1358 host walk already start from the function node, so they then pick it up; also feed is_bracketed into optionality (`pose?: number`, `| undefined`). Pitfall: leading JSDoc of an arrow that is a statement initializer is currently found via the VariableStatement host — don't double-apply; reparser attaches comment to the innermost node that can host it. contravariant...Function 0:6-0:11 lines (fn/bar/obj gaps, a) are the arrow `/** @param {string} a */ (a) => {}` after `fn:`.
- **Finished alone (2):** compiler/arrowFunctionJSDocAnnotation, compiler/contravariantOnlyInferenceWithAnnotatedOptionalParameterJs
- **Also blocked (other clusters needed) (1):** compiler/contravariantOnlyInferenceFromAnnotatedFunctionJs (+JSDOC-GATHER-TYPE-PARAMETERS)

### 71. `GENERIC-ARG-NIL-CONTEXTUAL-SIGNATURE` (V; blocked 3, finished alone 2, lines 18; confidence high)

- **Root cause:** For a context-sensitive arg whose parameter type is a type parameter (apparent type with zero or several call signatures), getContextualSignature/getContextualCallSignature return nil so the arrow is typed non-contextually and inferred into T; TSR's Absent arm refuses whenever single_generic_argument_context holds.
- **tsgo:** internal/checker/checker.go:10305 getContextualCallSignature (nil unless exactly one applicable sig) / :10264 getContextualSignature
- **TSR:** crates/tsr-checker/src/signatures.rs:6214 get_type_of_function_expression (ContextualSignature::Absent => !single_generic_argument_context)
- **Examples:**
  - `conformance/functionConstraintSatisfaction`: `(x) => x`. want `(x: any) => any`, got `error`. foo<T extends Function>(x: T) with foo((x)=>x): apparent Function has no call signature -> nil; TSR Absent arm declines in a single-generic-argument context (probe `bar<T>(x:T)` same).
  - `conformance/genericCallWithOverloadedFunctionTypedArguments`: `foo7(1, (x) => x)`. want `{ (x: any): string; (x: any, y?: any): string; }`, got `{ (x: number): string; (x: number, y?: number): string; }`. Contextual type has two call signatures -> getContextualCallSignature nil -> x:any, inferred T=any; TSR picks number.
- **Port plan:** Make the Absent arm answer the non-contextual signature (implicit any) like checker.go:10395 and ensure check_generic_call (inference.rs) infers from it in the context-sensitive pass. Pitfall from comment 6207-6213: overload-failure path (inferSignatureInstantiationForOverloadFailure SkipContextSensitive) must exist so error candidates still instantiate; r12 needs getContextualCallSignature's 'more than one applicable signature -> nil' rule ported instead of choosing one.
- **Finished alone (2):** conformance/functionConstraintSatisfaction, conformance/genericCallWithOverloadedFunctionTypedArguments
- **Also blocked (other clusters needed) (1):** conformance/typeParameterAsTypeParameterConstraintTransitively (+OBJLIT-PROPERTY-NAME-CONTEXTUAL-LITERAL)

### 72. `LATE-BOUND-SIGNATURES-OF-SYMBOL-IMPL-SKIP` (V; blocked 3, finished alone 2, lines 13; confidence high)

- **Root cause:** Late-bound method overloads merge into one late symbol whose signatures come from getSignaturesOfSymbol, which skips an implementation body immediately following a same-kind declaration; TSR's §383 sibling walk collects every sibling's signature (deduping only identical printed forms) and keeps the implementation.
- **tsgo:** internal/checker/checker.go:19806 getSignaturesOfSymbol (implementation skip :19818-19823) over the symbol built by :16005 lateBindMember
- **TSR:** crates/tsr-checker/src/symbols.rs:2732 get_type_of_func_class_enum_module_worker (`late_bound_overloads` block :2799-2857)
- **Examples:**
  - `conformance/symbolProperty40`: `c[Symbol.iterator]`. want `{ (x: string): string; (x: number): number; }`, got `{ (x: string): string; (x: number): number; (x: any): any; }`. Three `[Symbol.iterator]` declarations late-bind to one symbol; the third has a body and immediately follows a MethodDeclaration, so getSignaturesOfSymbol drops it. TSR's sibling walk pushes all three signatures (its printed-form dedupe cannot remove a distinct implementation).
  - `conformance/symbolDeclarationEmit3`: `[Symbol.toPrimitive]`. want `{ (x: number): any; (x: string): any; }`, got `{ (x: number): any; (x: string): any; (x: any): void; }`. Same rule with `[Symbol.toPrimitive]`; the `(x: any) { }` implementation is adjacent to the overloads and is skipped upstream but kept by TSR.
- **Port plan:** In the late_bound_overloads block, replace the printed-form dedupe with getSignaturesOfSymbol's rule applied over the sibling declarations in source order: skip declaration i>0 that has a body when declaration i-1 has the same parent and kind and ends exactly where it starts (pos == previous.end). Better: have late_bound_members_of produce a real merged late symbol per (name, static) and feed its declarations to the existing get_signatures_of_symbol_for_type, deleting the ad-hoc walk. Pitfalls: symbolProperty42 keeps `(x: any): any` because a STATIC overload sits between the instance overload and the impl (not adjacent) — the static member is in a different late table but still breaks adjacency; overloadsWithComputedNames' `["foo"](): void ... ["foo"]() {}` currently relies on the identical-print dedupe and must come out of the adjacency rule instead (check it does: non-adjacent there, so tsgo keeps both and the 'identical collapse' may be a different upstream mechanism — re-verify that case after the change); interface overloads with identical signatures (I1 in overloadsWithComputedNames) must NOT be collapsed. symbolProperty41's [0:30] also needs OVERLOAD-REORDER (other family).
- **Finished alone (2):** conformance/symbolDeclarationEmit3, conformance/symbolProperty40
- **Also blocked (other clusters needed) (1):** conformance/symbolProperty41 (+REORDER-CANDIDATES)

### 73. `GLOBALTHIS-SYMBOL-IN-GLOBALS` (V; blocked 3, finished alone 2, lines 8; confidence high)

- **Root cause:** NewChecker creates globalThisSymbol (Module, exports = globals) and inserts it into globals; TSR has no such symbol, only an identifier-expression special case, so name resolution (`import mod = globalThis`, `export { globalThis as global }`) and the `globalThis` member of typeof globalThis fail.
- **tsgo:** internal/checker/checker.go:962 NewChecker globalThisSymbol setup (962-964)
- **TSR:** crates/tsr-checker/src/expressions.rs:894 identifier `globalThis` special case (global_this_type); binder globals lack the symbol
- **Examples:**
  - `compiler/globalThisDeclarationEmit3`: `mod`. want `typeof mod`, got `any`. `import mod = globalThis`: the resolve_alias Identifier arm calls binder.resolve_name, which finds no globalThis, giving None and any.
  - `conformance/globalThisTypeIndexAccess`: `w_e`. want `typeof globalThis`, got `error`. `(typeof globalThis)['globalThis']`: the minted global_this_type has no globalThis member.
- **Port plan:** Create a real globalThis module symbol whose exports are the merged globals table, and register it in globals so binder resolve_name's globals fallback finds it. Make the identifier arms (expressions.rs:894, :1893) return get_type_of_symbol(globalThisSymbol) and delete global_this_type. Delete the any-fallthrough in types_producer.rs:717-723 that exists only for globalThis. Pitfall: tsgo marks it CheckFlagsReadonly, and typeof globalThis must print as `typeof globalThis`.
- **Finished alone (2):** conformance/globalThisGlobalExportAsGlobal, conformance/globalThisTypeIndexAccess
- **Also blocked (other clusters needed) (1):** compiler/globalThisDeclarationEmit3 (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 74. `RESOLVE-NAME-EXPORTED-IMPORT-EQUALS` (V; blocked 3, finished alone 2, lines 7; confidence high)

- **Root cause:** NameResolver's module-exports lookup uses getSymbol with meaning&ModuleMember, which accepts an ALIAS export whose target carries the meaning; TSR's binder exports arm admits exported import-equals aliases only for QualifiedName or require references, so `export import a = A` is unreachable by name inside its namespace.
- **tsgo:** internal/binder/nameresolver.go:138 Resolve (module exports lookup, meaning&SymbolFlagsModuleMember)
- **TSR:** crates/tsr-binder/src/lib.rs:802 resolve_name_excluding_with_export_alias exports arm (ImportEqualsDeclaration admission 828-843)
- **Examples:**
  - `conformance/circularImportAlias`: `a`. want `typeof a`, got `any`. `namespace B { export import a = A; export class D extends a.C }` — `a` is in B's exports as ALIAS with an Identifier reference, and the arm's match falls to `_ => {}`. Probe: `export import a = NS; var z = a` gives error, while the non-exported `import b = NS` works.
  - `compiler/es6ModuleInternalNamedImports`: `M_A`. want `typeof M_M`, got `any`. `export {M_A as a}` resolves M_A through export_specifier_target → binder.resolve_name, which misses the exported alias M_A = M_M. Probe `export { a as aa }` gives any.
- **Port plan:** In the exports arm, admit ALIAS exports with a ModuleReference::Identifier reference the same way as ExternalModuleReference: through the `exported_alias(found, meaning & mask)` checker callback, i.e. the alias chain's flags intersect the meaning. Better, collapse the per-kind match into that one callback for every ImportEquals alias, since that is upstream's getSymbol. Keep the pure-ExportSpecifier/NamespaceExport exclusion (nameresolver.go:133). Check the AMBIENT gate at 823 against upstream, which has no such gate.
- **Finished alone (2):** compiler/es6ModuleInternalNamedImports, compiler/es6ModuleInternalNamedImports2
- **Also blocked (other clusters needed) (1):** conformance/circularImportAlias (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 75. `AMBIENT-MODULE-SPECIFIER-MULTIDECL` (V; blocked 3, finished alone 2, lines 6; confidence high)

- **Root cause:** getSpecifierForModuleSymbol's ambient half spells any module symbol with no SourceFile declaration as its stripped name; TSR's type_to_string_at_worker only spells `typeof import("x")` when the ambient module has exactly ONE declaration, so a re-opened ambient module (2+ `declare module "x"` blocks) prints error.
- **tsgo:** internal/checker/nodebuilderimpl.go:1249 getSpecifierForModuleSymbol (ambient arm :1260-1263)
- **TSR:** crates/tsr-checker/src/checker.rs:1869 type_to_string_at_worker (gate `if let [declaration] = declarations` at :2150)
- **Examples:**
  - `compiler/ambientExternalModuleReopen`: `"fs"`. want `typeof import("fs")`, got `error`. Two instantiated `declare module "fs"` blocks in one script merge into a ValueModule with 2 declarations; tsgo prints the stripped ambient name. TSR's printer gate requires a single declaration (probe p4.ts `"m"` twice: error vs oracle typeof import("m")).
  - `compiler/exportSpecifierAndExportedMemberDeclaration`: `"m2"`. want `typeof import("m2")`, got `error`. Same: `declare module "m2"` declared twice (both instantiated by functions).
- **Port plan:** Relax the gate to upstream's test: if no declaration is a SourceFile (and no export=-container equivalent file) and the symbol name is an ambient module name (quoted), print `typeof import(<stripped name>)`. The comment at :2144-2147 cites moduleAugmentationExtend* bare-name wants as the reason for the gate; those modules merge a FILE module or are reached by an alias via module_alias_at (checked first at :2148), so the relaxed arm should still be preceded by the alias lookup — re-measure those cases. Note ambientDeclarationsPatterns also needs pattern ambient modules for its user.ts lines (other family).
- **Finished alone (2):** compiler/ambientExternalModuleReopen, compiler/exportSpecifierAndExportedMemberDeclaration
- **Also blocked (other clusters needed) (1):** conformance/ambientDeclarationsPatterns (+PATTERN-AMBIENT-MODULE)

### 76. `TYPE-PARAMETER-CONSTRAINT-NODE-REUSE` (V; blocked 3, finished alone 2, lines 5; confidence high)

- **Root cause:** typeParameterToDeclaration prints the constraint by reusing the written constraint node whenever getTypeFromTypeNode(constraintNode) == constraint (typeToTypeNodeHelperWithPossibleReusableTypeNode). TSR's written_constraint uses written_annotation_text heuristics plus a tuple arm, so `Q extends FilterQuery` (a keyof alias) and lib freeze's type-literal constraint print re-rendered.
- **tsgo:** internal/checker/nodebuilderimpl.go:1611 typeParameterToDeclaration (-> :1597 typeToTypeNodeHelperWithPossibleReusableTypeNode)
- **TSR:** crates/tsr-checker/src/signatures.rs:5754 type_parameter_of (written_constraint at :5800)
- **Examples:**
  - `compiler/divideAndConquerIntersections`: `matchFilter`. want `<U extends Update, Q extends FilterQuery>(filter: Q \| Q[]) => ...`, got `<U extends Update, Q extends "callback_query" \| ... >(filter: Q \| Q[]) => ...`. `type FilterQuery = keyof Omit<Update,"update_id">` gives a union with no alias attached. tsgo reuses the written `FilterQuery` node because its type == the constraint; TSR's written_annotation_text has no arm for this, so it prints the expanded union.
  - `compiler/contextualSignatureInObjectFreeze`: `Object.freeze`. want `<T extends { [idx: string]: U \| null \| undefined \| object; }, ...>`, got `<T extends { [idx: string]: object \| U \| null \| undefined; }, ...>`. es5.d.ts:210 writes the constraint literal in source order, and tsgo reuses the written node. TSR re-renders the semantic union in its interned order; ad-hoc c2 probe confirmed against the tsgo oracle.
- **Port plan:** In type_parameter_of, set written_constraint from the reused node whenever get_type_from_type_node(constraint) == the resolved constraint and the constraint is not error. That is always true for an uninstantiated declaration, so in practice: always the written text, unless it cannot be reused. Use the same faithful node-reuse printer as SIGNATURE-DECLARATION-ANNOTATION-REUSE, and drop the tuple special case. Keep inference.rs:5850's clearing when instantiation changes the constraint. Pitfall: only the uninstantiated typeParameter declaration reuses; an instantiated fresh type parameter's constraint differs and must render.
- **Finished alone (2):** compiler/contextualSignatureInObjectFreeze, compiler/inlinedAliasAssignableToConstraintSameAsAlias [type-operators]
- **Also blocked (other clusters needed) (1):** compiler/divideAndConquerIntersections (+CONDITIONAL-TYPE-ROOT-ALIAS, PREFIX-NOT-TYPE-FACTS, unaligned)

### 77. `UNTYPED-CALL-FUNCTION-TYPED-CALLEE` (V; blocked 3, finished alone 2, lines 4; confidence high)

- **Root cause:** isUntypedFunctionCall's third disjunct - a non-union, non-never callee with no call/construct signatures that is assignable to globalFunctionType (e.g. typed `Function`) is an untyped call -> any; TSR ports only the any disjunct (and the Function arm only for module_value_clones).
- **tsgo:** internal/checker/checker.go:9936 isUntypedFunctionCall (numCallSignatures==0 && numConstructSignatures==0 && !union && !never && isTypeAssignableTo(funcType, globalFunctionType))
- **TSR:** crates/tsr-checker/src/calls.rs:2113 is_untyped_call_target (doc at calls.rs:2091-2095 lists this arm as unported; reuse the Function-assignability code at calls.rs:636-660 in check_call_expression_worker)
- **Examples:**
  - `compiler/functionType`: `(new Function("return 5"))()`. want `any`, got `error`. `new Function(...)` is `Function`, which has no call signatures but is assignable to globalFunctionType, so tsgo resolves untyped -> any. TSR's gate requires an any-flagged callee and returns false for Function, then resolve_call_signature finds no signature -> error (probe `declare const fn: Function; fn()` error, tsgo any).
  - `compiler/unionOfFunctionAndSignatureIsCallable`: `c1()`. want `any`, got `error`. Parameter `c1: Function` -> same third disjunct -> any; TSR gaps c1() and the gap propagates to `const a = c1()` (line 0:6 `a`).
- **Port plan:** Add the third disjunct to is_untyped_call_target (or directly in check_call_expression_worker before resolve_call_signature_at): callee apparent type has zero call AND zero construct signatures, is not a union, reduced type is not never, and relate_ternary(callee, global Function declared type, Assignable)==Related - the module_value_clones block at calls.rs:636-660 already computes exactly this Function assignability and should be folded into the shared predicate (delete the special case). Signature counts must come from a complete query (call_signatures_of_type answering None for 'unresolved' must NOT be read as zero - require member completeness, see signatures.rs:6255-6268 caveat). Also add the TypeParameter disjunct (IsTypeAny(apparent) && funcType is type parameter) while there. Unions are excluded upstream - c3 `callable()` on `typeof c1 | typeof c2` must keep resolving normally.
- **Finished alone (2):** compiler/functionType, conformance/callWithSpreadES6
- **Also blocked (other clusters needed) (1):** compiler/unionOfFunctionAndSignatureIsCallable (+SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### 78. `EXPANDO-ASSIGNMENT-LATE-BIND-MEMBER` (V; blocked 2, finished alone 2, lines 36; confidence high)

- **Root cause:** Expando element assignments `foo[c] = v` with a dynamic name are bound as __computed and recorded on foo's __assignmentDeclaration export, then late-bound into foo's resolved exports by lateBindMember; TSR's binder drops dynamic-name expando assignments and callable_export_properties reads only early-bound exports.
- **tsgo:** internal/checker/checker.go:15962 getResolvedMembersOrExportsOfSymbol (assignment-declaration arm) -> :16005 lateBindMember; binder internal/binder/binder.go:1057 bindDeferredExpandoAssignment HasDynamicName arm -> :1000 addLateBoundAssignmentDeclarationToSymbol
- **TSR:** crates/tsr-checker/src/callable_expandos.rs:16 callable_export_properties (+ crates/tsr-binder/src/binder.rs:3535 bind_deferred_expando_assignment, `let name = name?` at :3559)
- **Examples:**
  - `compiler/declarationEmitLateBoundAssignments`: `foo[_private]`. want `string`, got `error`. `const _private = Symbol(); foo[_private] = "ok";` is an expando with a unique-symbol late-bindable name; tsgo late-binds it as foo's export `[_private]: string`. TSR's binder returns at `let name = name?` for the computed name, so foo has no such export and the read errors.
  - `compiler/declarationEmitLateBoundAssignments`: `foo`. want `{ (): void; bar: number; [_private]: string; strMemName: string; "dashed-str-mem": string; 42: string; }`, got `{ (): void; bar: number; }`. Same missing late-bound exports seen through the printed callable type: callable_export_properties iterates only `exports` (only `bar`). The JS twin declarationEmitLateBoundJSAssignments fails identically (not an independent witness).
- **Port plan:** Binder: in bind_deferred_expando_assignment, when the access name is dynamic (element access whose argument is not a string/numeric literal), bind the node anonymously as PROPERTY|ASSIGNMENT named __computed and append it to the target symbol's `__assignmentDeclaration` export (new internal name) instead of returning. Checker: port getResolvedMembersOrExportsOfSymbol's static/exports half for function/expando symbols — for each __assignmentDeclaration declaration with hasLateBindableName (entity-name argument with literal/unique-symbol type) call a lateBindMember equivalent keyed by getPropertyNameFromType, merged with early exports — and make callable_export_properties (and property lookup on the function's type) read that resolved table, skipping the __assignmentDeclaration entry itself. Pitfalls: ordering of members must follow declaration order (`bar`, `[_private]`, `strMemName`, `"dashed-str-mem"`, `42`); printed names: unique symbol -> `[_private]`, string -> quoted if not identifier, number -> bare; exclude __assignmentDeclaration from the VALUE-filtered exports iteration; declarationEmitLateBoundAssignments2 (outside this group) is the same mechanism with more name shapes and will move too.
- **Finished alone (2):** compiler/declarationEmitLateBoundAssignments, compiler/declarationEmitLateBoundJSAssignments

### 79. `SYMBOL-NAME-AS-WRITTEN-SOURCE-TEXT` (V; blocked 2, finished alone 2, lines 19; confidence high)

- **Root cause:** getNameOfSymbolAsWritten prints a symbol name as scanner.DeclarationNameToString(declaration name), i.e. the declaration's SOURCE text, which keeps unicode escapes like `C\u0032`. TSR prints the cooked identifier text / symbol name (`C2`).
- **tsgo:** internal/checker/nodebuilderimpl.go:973 getNameOfSymbolAsWritten (:1002 DeclarationNameToString)
- **TSR:** crates/tsr-checker/src/declared.rs:6291 new_named_type (declared_name from identifier.text at :6327); value side crates/tsr-checker/src/checker.rs:2255 value_symbol_name_at
- **Examples:**
  - `conformance/parserClassDeclaration23`: `C\u0032`. want `C\u0032`, got `C2`. The class instance type's name comes from the declaration's source text. new_named_type reads Identifier.text, which the scanner has unescaped; probe pn.ts `class C\u0032` prints `C2`, and `typeof C2` for the value.
  - `compiler/escapedIdentifiers`: `moduleType\u0032`. want `typeof moduleType\u0032`, got `typeof moduleType2`. Same rule on the value/typeof path: the namespace symbol prints its declaration name's source text.
- **Port plan:** Add a getNameOfSymbolAsWritten port that returns the source slice of the first named declaration's name node (node span over the file text), not the cooked text or symbol.name, with upstream's `default` and computed-name arms (:978-1001). Use it in new_named_type (class/interface/alias/enum names) and in value-name printing (value_symbol_name_at, best_name/symbol_chain segment text). Pitfall: escaped names still bind and compare by cooked text. Only the display string changes. best_name's table lookups must keep comparing on symbol.name.
- **Finished alone (2):** compiler/escapedIdentifiers, conformance/parserClassDeclaration23

### 80. `SYMBOLCONSTRUCTOR-UNIQUE-SYMBOL-COMPAT` (V; blocked 2, finished alone 2, lines 16; confidence high)

- **Root cause:** widenTypeForVariableLikeDeclaration turns an ESSymbol-typed member declared in the global SymbolConstructor into a unique symbol (getESSymbolLikeTypeForNode); TSR keeps `symbol`.
- **tsgo:** internal/checker/checker.go:18246 widenTypeForVariableLikeDeclaration (isGlobalSymbolConstructor arm; func :18242, helper :18236, :22982 getESSymbolLikeTypeForNode)
- **TSR:** crates/tsr-checker/src/symbols.rs:4247 get_widened_type_for_variable_like_declaration
- **Examples:**
  - `conformance/symbolProperty61`: `Symbol.obs`. want `unique symbol`, got `symbol`. `interface SymbolConstructor { readonly obs: symbol }` -> special arm makes it unique; probe sym.ts gives symbol. [0:38] from(...) is downstream (late-bound [Symbol.obs] key).
  - `compiler/symbolObserverMismatchingPolyfillsWorkTogether`: `observer`. want `unique symbol`, got `symbol`. Same arm for `readonly observer: symbol` merged into SymbolConstructor.
- **Port plan:** At the start of get_widened_type_for_variable_like_declaration, if the type has ESSYMBOL flag and the declaration's parent symbol is the global SymbolConstructor type symbol, replace with the unique-symbol type for the declaration (port getESSymbolLikeTypeForNode; reuse TSR's existing unique-symbol minting for `const x: unique symbol`). Verify property signatures in interfaces route through this function; if not, call it from the property-signature type path. Then late-bound `[Symbol.obs]` member names should resolve, fixing symbolProperty61 0:38.
- **Finished alone (2):** compiler/symbolObserverMismatchingPolyfillsWorkTogether, conformance/symbolProperty61

### 81. `JSDOC-TYPE-TAG-HOSTING` (V; blocked 2, finished alone 2, lines 11; confidence high)

- **Root cause:** reparseHosted moves a JSDoc @type onto the Type of any hostable declaration (PropertyDeclaration, ExportAssignment, PropertyAssignment, GetAccessor, VariableDeclaration), so it is both the declared type and the contextual type of the initializer; TSR reads @type only for VariableDeclaration's declared type (jsdoc_type_annotation) and never as a contextual type.
- **tsgo:** internal/parser/reparser.go:342 reparseHosted (KindJSDocTypeTag arm, PropertyDeclaration/ExportAssignment case :356)
- **TSR:** crates/tsr-checker/src/symbols.rs:5693 jsdoc_type_annotation (VariableDeclaration-only gate :5697) + crates/tsr-checker/src/contextual.rs:1084 get_contextual_type VariableDeclaration arm (reads r#type only)
- **Examples:**
  - `conformance/typeFromPrivatePropertyAssignmentJs`: `#a`. want `{ foo?: string; } \| undefined`, got `any`. `/** @type {{foo?: string}\|undefined} */ #a;` in a JS class; reparser sets PropertyDeclaration.Type. Probe: `/** @type {number} */ q;` and `#p` both any in TSR — jsdoc_type_annotation returns None for non-VariableDeclaration.
  - `compiler/checkJsdocTypeTagOnExportAssignment8`: `b`. want `"b"`, got `string`. `/** @type {Foo} */ export default {a:'a', b:'b'}`: Type on ExportAssignment contextually types the literal so 'b' stays literal. Probe: `/** @type {{a:string,b:"b"}} */ const z = {...}` gives z right but initializer `b: string`, i.e. contextual arm ignores JSDoc (case also needs JSDOC-BARE-TYPEDEF-TYPE-LITERAL).
- **Port plan:** Generalise jsdoc_type_annotation into an effective-type-annotation helper mirroring reparseHosted's parent list (VariableDeclaration via statement/declaration, PropertyDeclaration incl. private names, PropertyAssignment, ShorthandPropertyAssignment, GetAccessor, ExportAssignment), use it from type_annotation_of-style declared-type paths and from get_contextual_type's VariableDeclaration/PropertyDeclaration/ExportAssignment arms (checker.go:29423 getContextualTypeForInitializerExpression). Pitfall: ExportAssignment declared type is deliberately gapped (see JS-DEFAULT-EXPORT-JSDOC-TYPE); contextual typing alone fixes ExportAssignment8.
- **Finished alone (2):** compiler/checkJsdocTypeTagOnExportAssignment8, conformance/typeFromPrivatePropertyAssignmentJs

### 82. `JSDOC-BARE-TYPEDEF-TYPE-LITERAL` (V; blocked 2, finished alone 2, lines 11; confidence high)

- **Root cause:** A `@typedef Name` with no {type} (or {Object}) followed by @property tags is reparsed into a type alias whose body is a JSDocTypeLiteral; TSR's binder only creates the member-owning alias when the typedef has an explicit {Object}/{object} type expression, and type_alias_body returns None for a missing type expression.
- **tsgo:** internal/parser/reparser.go:240 reparseJSDocTypeLiteral (via reparseUnhosted typedef arm reparser.go:70)
- **TSR:** crates/tsr-binder/src/binder.rs:2984 bind_jsdoc_declarations (property_owner requires typedef.type_expression at :3016) + crates/tsr-checker/src/declared.rs:6212 type_alias_body
- **Examples:**
  - `compiler/strictOptionalProperties4`: `x`. want `Foo`, got `{}`. `@typedef Foo` + `@property {number} [foo]` then `/** @type {Foo} */ ({})`; tsgo alias Foo = {foo?: number}. TSR binder bails at binder.rs:3016 (no type expression) so the alias has no members and the cast is ignored.
  - `compiler/jsdocTypedef_propertyWithNoType`: `x`. want `Foo`, got `{ foo: number; }`. Same bare `@typedef Foo` with `@property foo`; probe with typed `@property {number} foo` also yields the initializer type, `{Object}` form works.
- **Port plan:** In bind_jsdoc_declarations treat a typedef with type_expression None (and with @property siblings) like the `{Object}` case: declare the alias and bind the @property tags as its members (upstream: typeExpression nil -> JSDocTypeLiteral of the property tags). Untyped @property gets any. Note checkJsdocTypeTagOnExportAssignment8 also uses a bare typedef and will need this plus JSDOC-TYPE-TAG-HOSTING.
- **Finished alone (2):** compiler/jsdocTypedef_propertyWithNoType, compiler/strictOptionalProperties4

### 83. `FOR-AWAIT-OF-BINDING-PARENT` (V; blocked 2, finished alone 2, lines 10; confidence high)

- **Root cause:** A binding pattern under `for await` takes its parent type from getTypeForVariableLikeDeclaration's ForOf arm (checkRightHandSideOfForOf, which handles async iteration); TSR's binding-parent copy of that arm explicitly excludes await_modifier and gaps.
- **tsgo:** internal/checker/checker.go:16664 getTypeForVariableLikeDeclaration (ForOfStatement arm -> checkRightHandSideOfForOf) via :17695 getTypeForBindingElementParent
- **TSR:** crates/tsr-checker/src/destructure.rs:496 get_type_for_binding_element_parent (for-of arm gated `for_of.await_modifier.is_none()` at :500)
- **Examples:**
  - `compiler/modularizeLibrary_Dom.asynciterable`: `key`. want `string`, got `error`. `for await (const [key, handle] of dir)`; the async-iterated element is [string, FileSystemHandle-union] and key indexes it. TSR's parent arm refuses await -> error; handle/handle.kind/kind (0:12-0:16) cascade.
  - `compiler/modularizeLibrary_Worker.asynciterable`: `handle`. want `FileSystemDirectoryHandle \| FileSystemFileHandle`, got `error`. Same source shape (near-duplicate fixture, not independent); minimal probe /tmp/triage/v_G11/m.ts: `for await (const [k, h] of arr)` k,h error while `for await (const e of arr)` is right.
- **Port plan:** Make the binding-parent path reuse the symbols.rs:5211-5240 for-of arm (which already computes for_await_of_yield_types for await) instead of its own sync-only copy: extract that arm into one helper (checkRightHandSideOfForOf equivalent) called from both get_type_for_variable_like_declaration and get_type_for_binding_element_parent, and delete the duplicated block at destructure.rs:491-511. Pitfall: the symbol arm widens the element (get_widened_literal_type) — upstream's parent read does not widen separately; check that the shared helper returns the unwidened iterated type for the parent and that the symbol arm keeps its widening.
- **Finished alone (2):** compiler/modularizeLibrary_Dom.asynciterable, compiler/modularizeLibrary_Worker.asynciterable

### 84. `JS-DEFAULT-EXPORT-JSDOC-TYPE` (V; blocked 2, finished alone 2, lines 6; confidence medium)

- **Root cause:** The `default` export of a JS ExportAssignment carrying a JSDoc @type (hosted or as a parenthesized cast) has the tag's type; TSR deliberately returns error for it because importing files must print the typedef alias module-qualified (`import("./a").NumberLike[]`), which TSR's printer does not do.
- **tsgo:** internal/parser/reparser.go:342 reparseHosted (ExportAssignment / ParenthesizedExpression -> makeNewCast :674); typed via getTypeOfVariableOrParameterOrPropertyWorker ExportAssignment arm
- **TSR:** crates/tsr-checker/src/symbols.rs:4000 ExportAssignment arm of the symbol-type worker (jsdoc_cast_annotation gates at :4008 and :4021)
- **Examples:**
  - `compiler/exportDefaultWithJSDoc2`: `A`. want `import("./a").NumberLike[]`, got `any`. `export default /** @type {NumberLike[]} */([ ])`; probe: the cast itself types `number[]` locally and a plain-identifier export of the same cast works, but symbols.rs:4021 returns error when the expression carries a JSDoc cast.
  - `compiler/exportDefaultWithJSDoc1`: `A[0]`. want `import("./a").NumberLike`, got `error`. `/** @type {NumberLike[]} */ export default ([ ])` hits the :4008 gate; A[0] is downstream.
- **Port plan:** Remove both gates and type the tag/cast (reuse JSDOC-TYPE-TAG-HOSTING helper for the hosted form). The real prerequisite is the type printer: a JSDoc typedef alias referenced from another file must print as `import("./a").NumberLike` (symbol accessibility / module-specifier qualification for non-exported JSDoc aliases). The comment records earlier drafts went G->W because the alias printed unqualified; land the printer change first.
- **Finished alone (2):** compiler/exportDefaultWithJSDoc1, compiler/exportDefaultWithJSDoc2

### 85. `JSDOC-TYPE-GRAMMAR` (V; blocked 2, finished alone 2, lines 6; confidence high)

- **Root cause:** parseJSDocType/parseNonArrayType parse JSDoc-only type syntax (`...T` variadic, `*` all-type, `?T`/`T?` nullable) inside {braces}; TSR's parse_jsdoc_type_expression hands off to the plain TS parse_type, so these nodes are never built and the annotation fails to any.
- **tsgo:** internal/parser/parser.go:2858 parseJSDocType (variadic :2866; JSDocAllType :2842; prefix nullable :2855; postfix :2728)
- **TSR:** crates/tsr-parser/src/jsdoc.rs:762 parse_jsdoc_type_expression (+ types.rs parse_non_array_type/parse_postfix_type in JSDoc context)
- **Examples:**
  - `compiler/jsdocTypeGenericInstantiationAttempt`: `list`. want `any[]`, got `any`. `@param {Array<*>} list`: tsgo parses `*` as JSDocAllType -> any so Array<any>. grep: no JSDocAllType/Variadic/Nullable constructor anywhere in tsr-parser (checker arms in declared.rs:585-610 are dead); probe `Array<?number>` also any, `Array<number>` fine.
  - `compiler/jsdocRestParameter_es6`: `a`. want `number[]`, got `any[]`. `@param {...number} a` on `...a`: tsgo builds JSDocVariadicType, rest param gets number[]; TSR parse fails so rest falls back to any[] (probe: `@param {number[]}` on `...a` gives number[]).
- **Port plan:** Port parseJSDocType: in the JSDoc type context (a jsdoc flag on the parser while inside parse_jsdoc_type_expression), accept leading `...` -> JSDocVariadicType, `*` -> JSDocAllType and `?` prefix/postfix -> JSDocNullableType in parseNonArrayType/parsePostfixType, `!` -> JSDocNonNullableType. Checker arms already exist (declared.rs:585-610). For a rest param whose JSDoc type is JSDocVariadicType, upstream's getTypeOfParameter yields the array (variadic arm creates Array<T>); ensure the rest-param path does not re-wrap into T[][].
- **Finished alone (2):** compiler/jsdocRestParameter_es6, compiler/jsdocTypeGenericInstantiationAttempt

### 86. `IMPORT-CALL-ARGUMENT-CONTEXT` (V; blocked 2, finished alone 2, lines 2; confidence high)

- **Root cause:** getContextualTypeForArgumentAtIndex's import-call arm (argument 0 of import(...) is contextually string, 1 is ImportCallOptions) is unported, so a `yield` inside import(...) has no contextual next type.
- **tsgo:** internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex (IsImportCall arm :29773-29781)
- **TSR:** crates/tsr-checker/src/contextual.rs:2348 contextual_type_for_argument
- **Examples:**
  - `conformance/importCallExpressionReturnPromiseOfAny`: `loadModule  (function* loadModule(directories: string[]) { ... import(yield path) })`. want `(directories: string[]) => Generator<string, void, string>`, got `error`. getContextualType(yield path) = string via the import-call arm feeds the NEXT slot. TSR declines the CallExpression parent in return_type_from_body and contextual_type_for_argument has no import arm (probe t6: tsgo AsyncGenerator<string, void, string>, TSR error).
- **Port plan:** Add the IsImportCall arm at the top of contextual_type_for_argument (index 0 -> string, 1 -> global ImportCallOptions type, else any); TSR's CallExpression node for import(...) must be recognised the way ast.IsImportCall does. Only reaches these baselines once YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE routes yields through get_contextual_type.
- **Finished alone (2):** compiler/asyncImportNestedYield, conformance/importCallExpressionReturnPromiseOfAny

### 87. `IMPORT-EQUALS-ALIAS-TYPEREF-TARGET` (V; blocked 2, finished alone 2, lines 2; confidence medium)

- **Root cause:** An UNQUALIFIED type reference through an import-equals alias (`var v: a`, `Foo<number>` with `import a = require(...)` of an `export = class C<T>`) goes through tsgo's resolveEntityName alias loop and then getTypeReferenceType(target). There the class arity check gives errorType for a bare generic, and `Foo<number>` instantiates. TSR's §157 arm mints the alias's spelling for argument-less references, skipping the arity check, and answers error for any reference with arguments.
- **tsgo:** internal/checker/checker.go:23146 getTypeReferenceType (reached after resolveEntityName's alias loop at :15821; arity error at :23197)
- **TSR:** crates/tsr-checker/src/declared.rs:1465 get_type_from_type_reference §157 ImportEquals alias mint (:1465-1505; argument-bearing references fall to :1620 with the alias symbol -> error at :1626)
- **Examples:**
  - `compiler/externalModuleExportingGenericClass`: `v`. want `any`, got `a`. `import a = require('./file0')` where file0 is `class C<T>{} export = C`; `var v: a` is a generic class without type args, so getTypeFromClassOrInterfaceReference reports TS2314 and returns errorType (printed any). TSR's §157 arm mints `a` without asking arity.
  - `compiler/privacyCheckExternalModuleExportAssignmentOfGenericClass`: `foo`. want `Foo<number>`, got `error`. `import Foo = require(...)` of `export = Foo; class Foo<A>`; `foo: Foo<number>` instantiates the class. TSR's §157 arm is argument-less only, so the reference falls through to the alias symbol (0 type params) and checkNoTypeArguments gives error.
- **Port plan:** Replace the §157 mint (declared.rs:1465-1505) with: resolve_alias the ImportEquals alias (meaning-filtered, see TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL), then run the shared get_type_reference_type(node, target) (identifier tail at declared.rs:1620-1651: arity window, get_instantiated_type_reference). Unresolvable or non-TYPE targets go to unresolved_type_reference, not error. Pitfall: §157 minted the alias spelling because `var v: IC` (alias IC of class C) must print `IC`. tsgo gets that from the node builder's alias-aware symbol chain, not from the type. Removing the mint regresses those lines unless printing consults the local alias (NB-SYMBOL-CHAIN). Measure the §157 R lines before cutting over.
- **Finished alone (2):** compiler/externalModuleExportingGenericClass, compiler/privacyCheckExternalModuleExportAssignmentOfGenericClass

### 88. `JS-RETURN-TYPE-UNION-OF-RETURNS` (V; blocked 2, finished alone 2, lines 2; confidence high)

- **Root cause:** getReturnTypeFromBody unions all return expression types (subtype reduction) regardless of file kind; TSR declines (None) whenever a JS function has 2+ distinct return types.
- **tsgo:** internal/checker/checker.go:20126 getReturnTypeFromBody (aggregation via :20259 checkAndAggregateReturnExpressionTypes)
- **TSR:** crates/tsr-checker/src/signatures.rs:3470 return_type_from_body (`many =>` arm's in_js_file decline; same decline at :3057, :3090, :3310)
- **Examples:**
  - `compiler/unreachableJavascriptUnchecked`: `unreachable`. want `() => 1 \| 2`, got `error`. `function unreachable(){ return 1; return 2; }` in .js; tsgo unions literal returns. Probe: even a reachable `if (b) return 1; return 2;` in .js gives `u2 : error`, so it is the explicit JS decline, not reachability.
- **Port plan:** Delete the in_js_file early returns in the multi-return arms (signatures.rs ~3057/3090/3310/3470). The comment says they guard against JSDoc @overload signatures not being modeled — re-measure after JSDOC-OVERLOAD-SIGNATURES lands, or keep the decline only when the function's JSDoc has @overload tags.
- **Finished alone (2):** compiler/unreachableJavascriptChecked, compiler/unreachableJavascriptUnchecked

### 89. `CONDITIONAL-DEFERRAL-GATE` (V; blocked 6, finished alone 1, lines 14; confidence high)

- **Root cause:** getConditionalType defers only when isDeferredType(checkType/extendsType) (isGenericType: type variables, index types, generic mapped/tuple), and for a generic check still resolves to the false branch when permissive instantiations are not assignable; TSR defers whenever the check type mentions any type parameter (incl. a signature's own <A>), e.g. T[], fn types, tuples containing T.
- **tsgo:** internal/checker/checker.go:24300 getConditionalType (checkTypeDeferred :24328, permissive false arm :24377); :24475 isDeferredType
- **TSR:** crates/tsr-checker/src/declared.rs:6815 evaluate_conditional_node (L6848-6852 mentions_registered_type_parameter gate) and :7252 evaluate_conditional_inference
- **Examples:**
  - `conformance/genericRestParameters2`: `T05`. want `[x: string, ...args: T]`, got `T05<T>`. Parameters<(x: string, ...args: T) => void>: check is an anonymous fn type → not generic → resolved. Probe c1.ts/c10.ts: ReturnType<() => T>, `T[] extends ReadonlyArray<infer I>` all stay deferred in TSR.
  - `compiler/deferredConditionalTypes`: `T4`. want `0`, got `T4<T>`. `[A<T>] extends [0, 0]`: tuple reference is not generic and arities differ (checkTuples off) → permissive test false → 0. TSR's gate defers (probe c1.ts q).
  - `compiler/arrayFlatNoCrashInference`: `arr.flat(depth)`. want `FlatArray<T, -1 \| 0 \| ... \| 20>[]`, got `FlatArray<T[], number>[]`. FlatArray's `Arr extends ReadonlyArray<infer InnerArr>` with Arr=T[]: T[] is not generic object → resolved; probe c10.ts v keeps deferred.
- **Port plan:** Replace the mentions_registered_type_parameter test at declared.rs:6848 with ported isDeferredType(checkType, checkTuples) / isGenericType (getGenericObjectFlags: type variables, generic index, generic mapped types, tuples only when checkTuples), and port the non-deferred branch: infer via inferTypes, then permissive-instantiation definitely-false arm (:24377) and restrictive true arm (:24415). Pitfall: signature types with their own type parameters (<A>() => A extends X ? 1 : 0, H_inline1) must not count as generic; keep alias-name attachment on resolved results consistent.
- **Finished alone (1):** compiler/conditionalTypeGenericInSignatureTypeParameterConstraint
- **Also blocked (other clusters needed) (5):** compiler/arrayFlatNoCrashInference [type-operators] (+PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED); compiler/arrayFlatNoCrashInferenceDeclarations [type-operators] (+PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED); compiler/deferredConditionalTypes (+MAPPED-AS-CLAUSE-INDEX-TYPE, unaligned); compiler/ramdaToolsNoInfinite (+INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); conformance/genericRestParameters2 (+SPREAD-ELEMENT-EMPTY-TUPLE)

### 90. `CONDITIONAL-TYPE-ROOT-ALIAS` (V; blocked 5, finished alone 1, lines 13; confidence high)

- **Root cause:** A reference to a conditional-bodied alias outside an alias declaration has no new alias, so getConditionalType's deferred arm labels the result with the conditional ROOT's alias instantiated through the mapper (Parameters<...>, Extract<...>, PerformQuery<...>, Evaluate<...>); TSR prints the written outer alias reference.
- **tsgo:** internal/checker/checker.go:24300 getConditionalType (deferred arm :24436 result.alias = c.instantiateTypeAlias(root.alias, mapper)); reached via :23641 getTypeAliasInstantiation(alias=nil) -> :22485 getConditionalTypeInstantiation
- **TSR:** crates/tsr-checker/src/declared.rs:4167 get_instantiated_type_reference (conditional-alias arm gated by in_alias_declared_position; otherwise falls to create_type_reference at :4377 which prints the written alias)
- **Examples:**
  - `compiler/indexedAccessKeyofNestedSimplifiedSubstituteUnwrapped`: `args`. want `Parameters<Extract<T[K], AnyFunction>>`, got `Params<T[K]>`. `...args: Params<T[K]>` with `type Params<T> = Parameters<Extract<T, AnyFunction>>`: the reference is not in an alias declaration, so instantiation of the deferred Parameters conditional takes root.alias = Parameters with instantiated args. TSR prints the written Params<T[K]> (oracle: g<T>(a: P2<T>) tsgo `Extract<T, Function>`, TSR `P2<T>`).
  - `compiler/divideAndConquerIntersections`: `middleware`. want `Middleware<PerformQuery<U, Combine<L1Fragment<Q>, Q>>>[]`, got `Middleware<Filter<U, Q>>[]`. `Array<Middleware<Filter<U, Q>>>`: Filter<U,Q> is a reference to an alias whose body is PerformQuery<...> (a conditional); with no new alias the deferred conditional is labelled PerformQuery<U, Combine<...>> by getConditionalType. TSR keeps Filter<U, Q>.
- **Port plan:** Depends on TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION. Conditional types need a ConditionalRoot carrying root.alias (the alias whose declaration wrote the conditional) and the deferred result must set alias = explicit alias ?? instantiateTypeAlias(root.alias, mapper) (checker.go:24433-24437). Then getTypeFromTypeAliasReference passes alias=nil for references outside alias declarations (and outside import-alias resolution) and the result prints the root alias. Delete the in_alias_declared_position gating at declared.rs:4167-4206 in favour of always instantiating. Pitfall: the tail-recursion loop resets alias=nil when it moves to a new root (:24420-24425), and extraTypes unions do not keep the alias.
- **Finished alone (1):** compiler/indexedAccessKeyofNestedSimplifiedSubstituteUnwrapped
- **Also blocked (other clusters needed) (4):** compiler/coAndContraVariantInferences3 (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/deeplyNestedMappedTypes (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/divideAndConquerIntersections (+PREFIX-NOT-TYPE-FACTS, TYPE-PARAMETER-CONSTRAINT-NODE-REUSE, unaligned); compiler/genericCallInferenceInConditionalTypes1 [type-operators] (+KEYOF-ANY-TYPE-NODE)

### 91. `INSTANTIATE-MAPPED-TYPE-ALIAS-DROP` (V; blocked 5, finished alone 1, lines 11; confidence high)

- **Root cause:** instantiateMappedType's homomorphic arm (mapTypeWithAlias) keeps the enclosing alias only for a UNION type-variable image; for a non-union object/array/generic image it calls instantiateAnonymousType(t, mapper, nil) so the result keeps only the mapped type's OWN alias instantiated (Readonly<...>, Partial<...>, CleanedGaps<...>) or none, whereas TSR names the result after the enclosing alias.
- **tsgo:** internal/checker/checker.go:22535 instantiateMappedType (:22565 instantiateAnonymousType(..., nil); :22570 mapTypeWithAlias -> :25554)
- **TSR:** crates/tsr-checker/src/mapped.rs:766 instantiate_mapped_type (+ alias naming in crates/tsr-checker/src/declared.rs:4006 get_instantiated_type_reference / :4349 instantiate_identity_mapped_alias)
- **Examples:**
  - `compiler/inferrenceInfiniteLoopWithSubtyping`: `ObjMapReadOnly`. want `Readonly<{ [key: string]: Readonly<T>; }>`, got `ObjMapReadOnly<T>`. `type ObjMapReadOnly<T> = Readonly<{...}>` instantiates Readonly's homomorphic mapped type with alias ObjMapReadOnly; T's image is a single object so mapTypeWithAlias calls instantiateAnonymousType with nil alias and the result inherits Readonly<...>. TSR mints ObjMapReadOnly<T> (oracle O2<T> -> Readonly<{ [k: string]: T; }>, o2 -> Readonly<{ [k: string]: number; }>).
  - `compiler/specedNoStackBlown`: `SpecObject`. want `Partial<{ [key in keyof INPUT]: SpecValue<INPUT[key], ROOTINPUT>; }>`, got `SpecObject<INPUT, ROOTINPUT>`. `SpecObject<I,R> = Partial<{[key in keyof I]: ...}>`: Partial's type variable maps to a generic (non-union) mapped type, so the SpecObject alias is dropped and Partial<...> prints. TSR keeps the enclosing alias name.
- **Port plan:** Depends on TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION (needs alias attribute + reference = instantiated body). Port instantiateMappedType's alias handling into mapped.rs instantiate_mapped_type: take an alias argument; when the homomorphic type variable's image differs, call mapTypeWithAlias(getReducedType(image), instantiateConstituent, alias) where only a union image builds getUnionTypeEx(..., alias) and each constituent goes through instantiateAnonymousType(t, prepend(typeVariable->s, m), nil) (result alias = instantiateTypeAlias(t.alias, mapper)); arrays/tuples go through instantiateMappedArrayType/instantiateMappedTupleType with no alias; otherwise instantiateAnonymousType(t, m, alias). Remove the get_named_union_type alias re-attachment at declared.rs:4378-4390 once mapTypeWithAlias owns it. Pitfall: nodebuilder must print a mapped type over a non-type-parameter constraint as `X extends infer T ? {[k in keyof T]: T[k]} : never` (declarationEmitMappedTypePreservesTypeParameterConstraint lines want that form).
- **Finished alone (1):** compiler/specedNoStackBlown [type-operators]
- **Also blocked (other clusters needed) (4):** compiler/computedTypesKeyofNoIndexSignatureType (+CONDITIONAL-GENERIC-SIGNATURE-EXTENDS, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION); compiler/declarationEmitMappedTypePreservesTypeParameterConstraint [type-operators] (+DEFERRED-TYPE-REFERENCE-ALIAS, RELATER-NONPRIMITIVE-INDEX-SIGNATURE, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, unaligned); compiler/inferrenceInfiniteLoopWithSubtyping (+SPECIFIER-FOR-MODULE-SYMBOL); compiler/ramdaToolsNoInfinite (+CONDITIONAL-DEFERRAL-GATE, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT)

### 92. `FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS` (V; blocked 4, finished alone 1, lines 6; confidence medium)

- **Root cause:** tsgo assigns contextual parameter types verbatim even when they mention outer (non-adopted) type parameters; TSR's get_type_of_function_expression 'grounded' gate returns error whenever a materialised contextual param type mentions any type parameter.
- **tsgo:** internal/checker/checker.go:10349 assignContextualParameterTypes (via :10152 contextuallyCheckFunctionExpressionOrObjectLiteralMethod)
- **TSR:** crates/tsr-checker/src/signatures.rs:6201 get_type_of_function_expression (grounded / mentions_any_type_parameter gate)
- **Examples:**
  - `compiler/returnTypeParameterWithModules`: `function (x) { return g(f(x)); }`. want `(x: D) => C`, got `error`. Returned from a function with declared return (x: D) => C (outer D,C); TSR gives x : D but the gate refuses the expression since D is a type parameter. Probe `function m2<U>(): (r: U) => U { return (x) => x }` -> error.
  - `compiler/genericRestTypes`: `(x, ..._) => x`. want `(x: string, ..._: T) => string`, got `any`. Contextual (x: string, ...rest: T) => void with outer T; rest type mentions T so gate declines.
- **Port plan:** Delete the `grounded` mentions_any_type_parameter requirement in get_type_of_function_expression (signatures.rs:6183-6221) for the Present arm and build the signature from the contextual one as checker.go:10366-10392 does (getRestTypeAtPosition for rest). Pitfall: the comment records +138 wrong when ungated (generatedContextualTyping) — those wrongs are where TSR's contextual signature materialises with the wrong mapper (non-fixing vs fixing inference context); fix contextual_signature_result instantiation (instantiateContextualType with the inference context's mapper) rather than keeping the gate. Land together with CONTEXTUAL-SIGNATURE-TYPEPARAM-ADOPTION so generic contextual signatures don't print without <T>.
- **Finished alone (1):** compiler/contextualSignatureInstantiation2
- **Also blocked (other clusters needed) (3):** compiler/correlatedUnions (+LITERAL-WIDENING, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, UNATTRIBUTED, unaligned); compiler/genericRestTypes (+NODEBUILDER-GET-REDUCED-TYPE, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/returnTypeParameterWithModules (+SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 93. `WIDEN-NULLABLE-TO-ANY` (V; blocked 3, finished alone 1, lines 19; confidence high)

- **Root cause:** getWidenedTypeWithContext's RequiresWidening arm turns Nullable (widening) types into any, recursively through array elements and function return types (non-strict `[null,null]`->any[], `() => null`->() => any); TSR widen_type_with_context has no nullable->any arm.
- **tsgo:** internal/checker/checker.go:18359 getWidenedTypeWithContext (Nullable->anyType arm at :18368)
- **TSR:** crates/tsr-checker/src/widening.rs:95 widen_type_with_context (and :157 widen_array_members)
- **Examples:**
  - `compiler/widenedTypes1`: `f`. want `any[]`, got `null[]`. @strict false `var f = [null, null]`; declaration widening maps the null element to any. TSR probe: `var a = [null, null]` -> null[].
  - `conformance/heterogeneousArrayLiterals`: `[() => base, () => null]`. want `(() => any)[]`, got `error`. Non-strict `() => null` return is widened to any (probe TSR `const q = () => null` -> `() => null`); with () => any the subtype reduction is trivial, but TSR is left reducing `() => Base` vs `() => null` and its removeSubtypes declines. Reassigned from SUBTYPE-REDUCTION-UNDECIDABLE.
- **Port plan:** Depends on NULL-WIDENING-TYPES-MISSING (needs the widening intrinsics / ContainsWideningType so RequiresWidening is computable). Then add upstream's first switch arm in widen_type_with_context: Any|Nullable with RequiresWidening -> any, before the object-literal/union/array arms, and make return-type inference (getReturnTypeFromBody -> getWidenedType) go through it. arrayLiteralWidened's `undefined[][]` also needs the nullable element of `[]`'s implicit undefinedWidening to widen. Gate strictly on the widening twin, never on plain null (strict mode and `var n: null` stay null).
- **Finished alone (1):** conformance/arrayLiteralWidened
- **Also blocked (other clusters needed) (2):** compiler/widenedTypes1 (+NULL-WIDENING-TYPES-MISSING); conformance/heterogeneousArrayLiterals (+ARRAY-LITERAL-SUBTYPE-REDUCTION)

### 94. `GENERATOR-ANNOTATION-ITERATION-TYPES` (V; blocked 3, finished alone 1, lines 6; confidence high)

- **Root cause:** checkYieldExpression's non-star arm reads the NEXT type from the annotated return type via union filter (checkGeneratorInstantiationAssignabilityToReturnType) + getIterationTypesOfGeneratorFunctionReturnType (iterable then iterator, structural), orElse any; TSR's next_type_of_annotated_generator only reads the 3rd written type argument or 3rd type-parameter default of a direct reference and answers error otherwise.
- **tsgo:** internal/checker/checker.go:10952 checkYieldExpression (returnType filter + :6224 getIterationTypesOfGeneratorFunctionReturnType -> :6265 getIterationTypesOfIterable / :6485 getIterationTypesOfIterator); :6216 getIterationTypeOfGeneratorFunctionReturnType
- **TSR:** crates/tsr-checker/src/declared.rs:191 next_type_of_annotated_generator (callers crates/tsr-checker/src/expressions.rs:3564)
- **Examples:**
  - `conformance/generatorReturnTypeIndirectReferenceToGlobalType`: `yield 0  (function* f1(): I1, interface I1 extends Iterator<0, 1, 2>)`. want `2`, got `error`. getIterationTypesOfIterable(I1) fails (no [Symbol.iterator]) so getIterationTypesOfIterator walks I1's inherited next(...[value: 2]) giving next 2. TSR reads the reference I1 syntactically, finds no 3rd argument/parameter and answers error (probe t7 identical).
  - `conformance/types.asyncGenerators.es2018.1`: `yield  (const x = yield; in nextType1(): { next(...args: [] \| [number \| PromiseLike<number>]): any })`. want `number \| PromiseLike<number>`, got `error`. Structural iterator: next parameter type gives TNext = number \| PromiseLike<number> (not awaited). TSR only accepts TypeReferenceNode annotations.
  - `conformance/generatorYieldContextualType`: `yield step  (showStep(): StepResultGenerator<QuickPickItem>, alias of Generator<..., any\|undefined> \| AsyncGenerator<...`. want `any`, got `error`. The union annotation is filtered to the Generator constituent, whose TNext is any. TSR's syntactic reader sees a 1-parameter alias and gaps.
- **Port plan:** Port getIterationTypesOfGeneratorFunctionReturnType (any -> all any; getIterationTypesOfIterable with GeneratorReturnType/AsyncGeneratorReturnType use, else getIterationTypesOfIterator on the type itself) and getIterationTypeOfGeneratorFunctionReturnType on top of the engine from the yield* clusters, plus checkGeneratorInstantiationAssignabilityToReturnType (createGeneratorType + assignability) for the union filter. In check_yield_expression: returnType = getReturnTypeFromAnnotation(fn); if union, filterType by the instantiation check; non-star result = getIterationTypeOfGeneratorFunctionReturnType(Next, returnType) orElse any. Delete next_type_of_annotated_generator (its 'parameter default' reading is a proxy for the fast path's resolved type arguments). The same helper is needed by CONTEXTUAL-RETURN-GENERATOR-FILTER and by contextual_generator_iteration_type, so land it once. Pitfall: `{}` and other non-iterator annotations must answer `any` (OrElse), not error.
- **Finished alone (1):** conformance/generatorReturnTypeIndirectReferenceToGlobalType
- **Also blocked (other clusters needed) (2):** conformance/generatorYieldContextualType (+APPEND-LOCAL-TYPE-PARAMETERS, CONDITIONAL-BRANCH-NAMED-UNION, CONTEXTUAL-RETURN-GENERATOR-FILTER, INDEXED-ACCESS-GENERIC-DEFER, ORIGIN-ENTRY-ORDER, REMOVE-SUBTYPES-UNDECIDABLE, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE, unaligned); conformance/types.asyncGenerators.es2018.1 (+ASYNC-YIELD-STAR-ITERATION-TYPES, CONTEXTUAL-RETURN-IIFE-ARM)

### 95. `SHADOWED-TYPEPARAM-RENAME` (V; blocked 3, finished alone 1, lines 5; confidence medium)

- **Root cause:** typeParameterToName with GenerateNamesForShadowedTypeParams renames a shadowing type parameter once (cached per type id) and every reference — declaration, constraint, parameter, nested conditional/indexed — uses the cached name; TSR renames by text substitution in only some positions.
- **tsgo:** internal/checker/nodebuilderimpl.go:1404 typeParameterToName (:1396 typeParameterShadowsOtherTypeParameterInScope)
- **TSR:** crates/tsr-checker/src/inference.rs:5583 rename_type_parameters_for_site / :5694 rename_own_type_parameters_for_print; printing.rs:126 allocate_type_parameter_name
- **Examples:**
  - `compiler/inferenceContextualReturnTypeUnion3`: `Reflect.set`. want `<T_1 extends object, P ...>(target: T_1, ..., value: P extends keyof T_1 ? T_1[P] : any ...)`, got `<T extends object, P ...>(target: T_1, ..., value: P extends keyof T ? T[P] : any ...)`. Rename applied to parameter type only; declaration and conditional text untouched.
  - `compiler/twiceNestedKeyofIndexInference`: `set`. want `K2 extends keyof T_1[K1_1]`, got `K2 extends keyof T[K1]`. Constraint text of another type parameter not re-rendered with renamed names.
- **Port plan:** Make the printer resolve every type-parameter reference through one per-print name map keyed by TypeId (render_type_parameter_names), populated by a port of typeParameterToName (resolveName at enclosing node; suffix _N counter), and render constraints/conditionals from types rather than stored text. Delete the text-substitution rename helpers once all positions go through the map. declarationEmitNestedGenerics also blocked by CONDITIONAL-TYPE-NODE.
- **Finished alone (1):** compiler/inferenceContextualReturnTypeUnion3 [type-operators]
- **Also blocked (other clusters needed) (2):** compiler/declarationEmitNestedGenerics (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); compiler/twiceNestedKeyofIndexInference (+CHOOSE-OVERLOAD-GENERIC-WALK)

### 96. `DECLARED-TYPE-OF-ALIAS` (V; blocked 3, finished alone 1, lines 3; confidence high)

- **Root cause:** tryGetDeclaredTypeOfSymbol has an Alias arm (getDeclaredTypeOfAlias = declared type of resolveAlias target). TSR's get_declared_type_of_symbol has none and answers error for alias symbols, so the RHS/export arms fall back to the value type.
- **tsgo:** internal/checker/checker.go:23678 tryGetDeclaredTypeOfSymbol (Alias case) -> checker.go:24094 getDeclaredTypeOfAlias
- **TSR:** crates/tsr-checker/src/declared.rs:5456 get_declared_type_of_symbol
- **Examples:**
  - `compiler/declarationEmitEnumReferenceViaImportEquals`: `Translation`. want `Translation`, got `typeof Translation`. `import { Translation } …; import K = Translation.TranslationKeyEnum` — the qualified left resolves (Namespace, dontResolveAlias) to the import alias, whose declared type is the interface Translation. TSR's :1244 arm gets the alias, get_declared_type_of_symbol(alias) returns error, and the value fallback gives typeof Translation.
  - `conformance/shadowedInternalModule`: `A`. want `number`, got `any`. `namespace b { export import A = a.A; export namespace A {} }` then `import any = b.A`: the leaf b.A is an Alias\|Namespace symbol, and the Alias arm gives the declared type of `type A = number`. TSR's :1304 leaf arm gets error from declared.rs and the value of an uninstantiated alias, which prints any.
  - `compiler/importDeclarationNotCheckedAsValueWhenTargetNonValue`: `exp`. want `exp`, got `any`. `import exp = dojox…common; export = exp`: the export arm resolves alias exp, whose declared type is interface common (printed via the alias as exp). TSR gets error and falls to value any.
- **Port plan:** Add the Alias case last in get_declared_type_of_symbol (declared.rs:5482, before the error else), matching tryGetDeclaredTypeOfSymbol's switch order: Class/Interface, TypeParameter, TypeAlias, Enum, EnumMember, then Alias. It returns get_declared_type_of_symbol(resolve_alias(symbol)) when the alias resolves, otherwise error, cached in declared_types. Pitfalls: the producer's alias-printing roads (§491/§493 in declared.rs ~1460-1575) and the type-only arm (:834) resolve aliases themselves and may double-handle. Alias cycles must answer error rather than recurse, which upstream gets from resolveAlias's unknownSymbol. Several callers rely on 'declared == error' to mean 'not a type' for alias symbols (for example the :1147 heritage/export arms), so measure the change globally. The printed name for an alias-reached declared type (`exp`, `Y`) belongs to NB-SYMBOL-CHAIN.
- **Finished alone (1):** compiler/importDeclarationNotCheckedAsValueWhenTargetNonValue
- **Also blocked (other clusters needed) (2):** compiler/declarationEmitEnumReferenceViaImportEquals (+QUALIFIED-NAME-LEFT-ALIAS-RESOLVE); conformance/shadowedInternalModule (+RHS-OF-IMPORT-OR-EXPORT-ASSIGNMENT-ARM, SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### 97. `PREFIX-NOT-TYPE-FACTS` (V; blocked 2, finished alone 1, lines 3; confidence high)

- **Root cause:** `!x` result comes from getTypeFacts(operand, Truthy|Falsy) which handles instantiable types via their constraint/base; TSR's hand-written negated_truthiness_type table returns error for type parameters / indexed access.
- **tsgo:** internal/checker/checker.go:10887 checkPrefixUnaryExpression ExclamationToken arm -> :30974 getTypeFacts
- **TSR:** crates/tsr-checker/src/expressions.rs:1463 negated_truthiness_type
- **Examples:**
  - `compiler/divideAndConquerIntersections`: `!up`. want `false`, got `error`. up: U extends object-ish; getTypeFacts via constraint is truthy-only -> false. Probe not.ts `!z` (T extends {id:number}) -> error.
  - `compiler/definiteAssignmentOfDestructuredVariable`: `!(a && b)`. want `boolean`, got `error`. operand T["b"]\|undefined; facts via constraint give both -> boolean; TSR table errors on instantiable member.
- **Port plan:** Replace the table with getTypeFacts(operand, Truthy|Falsy) (port getTypeFactsWorker arms incl. Instantiable -> getBaseConstraintOfType or unknownType, unions OR, intersections) and map: only Truthy -> false, only Falsy -> true, else boolean. Check whether TSR already has a type_facts port used by narrowing (flow.rs) and reuse it rather than a second table.
- **Finished alone (1):** compiler/definiteAssignmentOfDestructuredVariable
- **Also blocked (other clusters needed) (1):** compiler/divideAndConquerIntersections (+CONDITIONAL-TYPE-ROOT-ALIAS, TYPE-PARAMETER-CONSTRAINT-NODE-REUSE, unaligned)

### 98. `CONDITIONAL-GENERIC-SIGNATURE-EXTENDS` (V; blocked 2, finished alone 1, lines 2; confidence medium)

- **Root cause:** The Equals idiom `(<A>() => A extends X ? 1 : 0) extends (<A>() => A extends Y ? 1 : 0)` answers error in TSR even for concrete X/Y: the inline conditional in the generic signature's return is not instantiable and generic-signature relation (instantiateSignatureInContextOf) is missing.
- **tsgo:** internal/checker/checker.go:24300 getConditionalType (assignability of generic signatures via compareSignaturesRelated) 
- **TSR:** crates/tsr-checker/src/declared.rs:6815 evaluate_conditional_node
- **Examples:**
  - `compiler/conditionalEqualityTestingNullability`: `ShouldBe0`. want `0`, got `error`. Probe c11.ts/c10.ts: Eq<1,1> and Eq<string,string> → error in TSR (not deferral), tsgo resolves.
- **Port plan:** First land CONDITIONAL-INLINE-NODE-INSTANTIATION (the signature return conditional must instantiate), then ensure relating two generic signatures unifies type parameters (instantiateSignatureInContextOf in signaturesRelatedTo) and that identical deferred conditionals compare via isTypeIdenticalTo on ConditionalRoot. Re-probe Eq<1,1> after the inline fix to see what remains.
- **Finished alone (1):** compiler/conditionalEqualityTestingNullability
- **Also blocked (other clusters needed) (1):** compiler/computedTypesKeyofNoIndexSignatureType (+INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION)

### 99. `JS-EXPANDO-EMPTY-OBJECT-LITERAL` (V; blocked 1, finished alone 1, lines 26; confidence medium)

- **Root cause:** checkObjectLiteral returns an anonymous type over node.Symbol().Exports for an empty JS object literal whose symbol has expando members; TSR's check_object_literal has no such arm, so `const A = {}; A.x = ...` stays `{}`.
- **tsgo:** internal/checker/checker.go:13146 checkObjectLiteral (empty-literal-with-exports arm)
- **TSR:** crates/tsr-checker/src/objects.rs:857 check_object_literal
- **Examples:**
  - `compiler/ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge`: `A`. want `{ bar: typeof Q; }`, got `{}`. namespacey.js `const A = {}; A.bar = class Q {}`; binder (binder.rs:3507 is_expando_initializer) recognises the empty literal, but probe `const A = {}; A.n = 1` gives `A : {}` and `A.n : error`, so the checker never builds the type from the expando members.
- **Port plan:** Add checker.go:13146's arm at the top of check_object_literal: zero properties and the literal's (or its declaration's) symbol has exports -> anonymous type over those exports, flagged JS-literal (insert into js_literal_types). Verify where TSR's binder files expando members (variable symbol exports vs literal symbol) and read that table. Downstream module.exports/require/defineProperty lines in the case follow.
- **Finished alone (1):** compiler/ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge

### 100. `MAPPED-MEMBERS-LOWER-BOUND-KEY` (V; blocked 1, finished alone 1, lines 16; confidence high)

- **Root cause:** resolveMappedTypeMembers on a generic key constraint (Extract<keyof T,'b'>, keyof T|'c', keyof (T&U)) enumerates getLowerBoundOfKeyType; TSR mapped_member_keys returns None for any generic key.
- **tsgo:** internal/checker/checker.go:21021 getLowerBoundOfKeyType (called from resolveMappedTypeMembers :20894)
- **TSR:** crates/tsr-checker/src/mapped.rs:481 mapped_member_keys (L556/L583 signature_parameter_type_is_generic → None)
- **Examples:**
  - `conformance/mappedTypeConstraints`: `obj.b`. want `T["b"]`, got `error`. obj: Pick<T, Extract<keyof T,'b'>>; lower bound of the key is 'b' so member b exists with type T['b']; TSR declines generic keys (classifier probe: Pick<T,'b'> works).
- **Port plan:** Port getLowerBoundOfKeyType (index → keyof apparent type, conditional → distributive instantiation with lower bound, union/intersection mapping, template/string mapping arms) and use it in mapped_member_keys instead of declining generic keys; keep the generic-key template instantiation per key (T["b"]).
- **Finished alone (1):** conformance/mappedTypeConstraints

### 101. `KEYOF-RESOLVED-OPERAND-INDEX-TYPE` (V; blocked 1, finished alone 1, lines 11; confidence medium)

- **Root cause:** getTypeFromTypeOperatorNode is getIndexType(getTypeFromTypeNode(operand)) for ANY operand, but TSR's keyof node arm only resolves syntactic union/intersection operands, generic-alias references and bare type parameters, and answers error otherwise (any non-generic alias of a union: `keyof U`), and keyof of a generic alias reference whose mapped body distributes over a union argument also errors.
- **tsgo:** internal/checker/checker.go:22960 getTypeFromTypeOperatorNode -> :26680 getIndexType (union arm = intersection of constituent key sets)
- **TSR:** crates/tsr-checker/src/declared.rs:621 get_type_from_type_node keyof arm (fallthrough `None => error` at :737); crates/tsr-checker/src/declared.rs:7555 resolved_keyof_type_worker
- **Examples:**
  - `compiler/inferenceUnionOfObjectsMappedContextualType`: `key`. want `"someDate"`, got `string`. Contextual type RowRenderer<Entity> = RowRendererMeta<Entity>[keyof RowRendererMeta<Entity>] with Entity normalising to a union: tsgo distributes the homomorphic mapped type and keyof gives the common keys, so the literal keeps 'someDate'. TSR answers error for keyof there (oracle p6: `keyof E3` named union and `keyof Meta<E3>` both error in TSR, tsgo "id" \| "s"; inline `keyof (A\|B)` right), losin
- **Port plan:** Replace the shape-gated keyof arm at declared.rs:621-738 with getIndexType(get_type_from_type_node(operand)) dispatching through resolved_keyof_type (whose union/intersection arms at :7641/:7649 already mirror getIndexType), keeping the deferred-index mint only for instantiable operands (shouldDeferIndexType). The `keyof Meta<E3>` half additionally needs Meta<E3> to be the distributed union of mapped instantiations, i.e. INSTANTIATE-MAPPED-TYPE-ALIAS-DROP/TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION; the plain `keyof U` half is independent. Pitfall: index-type origin printing (`keyof V` for an aliased object, oracle k3) must survive.
- **Finished alone (1):** compiler/inferenceUnionOfObjectsMappedContextualType

### 102. `JSDOC-SCAN-UNICODE-ESCAPE` (V; blocked 1, finished alone 1, lines 8; confidence high)

- **Root cause:** ScanJSDocToken decodes `\uXXXX`/`\u{X}` escapes in JSDoc identifiers so `@param {number} a\u0061` names `aa`; TSR's scan_jsdoc_token has no backslash arm, so the name never matches.
- **tsgo:** internal/scanner/scanner.go:1490 ScanJSDocToken (`case '\\'` arm; func at :1418)
- **TSR:** crates/tsr-scanner/src/jsdoc.rs:75 scan_jsdoc_token
- **Examples:**
  - `compiler/unicodeEscapesInJSDoc`: `foo`. want `(a: number, aa: number) => void`, got `(a: number, aa: any) => void`. `@param {number} a\u0061` for param aa; probe confirms `c4(aa)` any. TSR scan_jsdoc_token's identifier arm stops at `\`, so the tag name never decodes to `aa`.
- **Port plan:** Add the backslash arm: peek a unicode escape, and if it decodes to an identifier start, scan the rest with the escape-aware identifier scanner (the main scanner already has one), storing the decoded text as the token value; make parse_jsdoc_identifier_name use the decoded value rather than the source slice.
- **Finished alone (1):** compiler/unicodeEscapesInJSDoc

### 103. `INSTANTIATE-ANONYMOUS-TYPE-CACHE-FIRST` (V; blocked 1, finished alone 1, lines 7; confidence high)

- **Root cause:** instantiateAnonymousType creates and caches the instantiated object type BEFORE its members are resolved (members are instantiated lazily), so a self-referential object literal type terminates; TSR instantiate_anonymous_properties instantiates every property eagerly and inserts the cache entry only at the end, so `let x = { bar: (t1: T1) => x }` recurses to the depth limit and answers error.
- **tsgo:** internal/checker/checker.go:22304 getObjectTypeInstantiation / :22458 instantiateAnonymousType
- **TSR:** crates/tsr-checker/src/inference.rs:5362 instantiate_anonymous_properties
- **Examples:**
  - `compiler/genericCallOnMemberReturningClosedOverObject`: `example<number>()`. want `{ foo: <T2>(t2: T2) => any; bar: (t1: number) => any; }`, got `any`. The return type `{foo: ..=> x; bar: (t1: T1) => x}` references itself via x; tsgo caches the instantiation before resolving members. TSR's eager per-property instantiate_type re-enters the same (id,map) key uncached (oracle probe: self-referential example<number>() -> TSR error, non-recursive example2<number>() right).
- **Port plan:** Insert a reserved instantiated type into instantiated_objects before instantiating properties (the pattern mapped.rs:777 and declared.rs:1769 already use: reserve then complete_object), or better port the lazy member resolution: record (target, mapper) on the new anonymous type and instantiate members on first member read (resolveAnonymousTypeMembers). Pitfall: the printed text is computed at creation in TSR, so a reserved placeholder must print the eventual structure; the recursive member prints `any` in the baseline (nodebuilder depth cut), not the placeholder.
- **Finished alone (1):** compiler/genericCallOnMemberReturningClosedOverObject

### 104. `EXPORT-ASSIGNMENT-CLASS-EXPRESSION-TARGET` (V; blocked 1, finished alone 1, lines 5; confidence medium)

- **Root cause:** getTargetOfAliasLikeExpression resolves `export = <class expression>` to the class expression's symbol (checkExpressionCached(expr).symbol); TSR's export_assignment_target handles only an Identifier, so `import Chunk = require('./mod1')` has no target and every use is any/error.
- **tsgo:** internal/checker/checker.go:14996 getTargetOfAliasLikeExpression (:14997 IsClassExpression arm), via :14976 getTargetOfExportAssignment
- **TSR:** crates/tsr-checker/src/symbols.rs:1546 export_assignment_target
- **Examples:**
  - `compiler/commonJsImportClassExpression`: `Chunk`. want `typeof Chunk`, got `any`. mod1.ts `export = class { chunk = 1 }`: the export= target is the class expression's symbol, so the import-equals alias Chunk has type typeof Chunk and `c: Chunk` has member chunk. TSR's export_assignment_target returns None for a non-Identifier expression; oracle jx2.ts reproduces (tsgo `Chunk : typeof Chunk`, `c.chunk : number`; TSR any/error).
- **Port plan:** Add the class-expression arm to export_assignment_target: if node.expression is a ClassExpression return binder.symbol_of(class expression) (equivalent of checkExpressionCached(expr).symbol). Confirm the binder marks `export = <class expression>` as an alias (upstream ExportAssignmentIsAlias accepts class expressions); if TSR's binder makes it a property instead, fix there. Pitfall: the class symbol's printed name must come from the alias (`Chunk`), not `(Anonymous class)` — check the alias-renaming path used for `export = Identifier` handles it.
- **Finished alone (1):** compiler/commonJsImportClassExpression

### 105. `STATIC-SIDE-FROM-BASE-CONSTRUCTOR-TYPE` (V; blocked 1, finished alone 1, lines 4; confidence high)

- **Root cause:** A class constructor type inherits static members via resolveAnonymousTypeMembers adding getPropertiesOfType(getBaseConstructorTypeOfClass(classType)); TSR's static lookup walks base symbols with base_symbols_of (refuse_type_arguments=true), so any base written with type arguments (`extends C2<T>`) or as an expression gaps.
- **tsgo:** internal/checker/checker.go:20650 resolveAnonymousTypeMembers (:20686-20690 getBaseConstructorTypeOfClass + addInheritedMembers)
- **TSR:** crates/tsr-checker/src/members.rs:2504 static_property_of_bases (base_symbols_of at :2510; also :3004 collect_static_property_names)
- **Examples:**
  - `conformance/classExtendingClass`: `D2.other`. want `<T>(x: T) => void`, got `any`. `class D2<T> extends C2<T>`, `static other<T>(x: T)` on C2: getBaseConstructorTypeOfClass(D2) = typeof C2 and its properties are added to typeof D2. TSR's static_property_of_bases calls base_symbols_of, which refuses entries with type arguments, so D2.other is a miss (r8 any downstream). Probe h.ts: `class H extends G<number>`, H.g TSR error / tsgo (x: number) => number; `extends o.A` statics also
- **Port plan:** Replace the base-symbol recursion in static_property_of_bases with get_property_of_type(get_base_constructor_type_of_class(owner), name) when the base constructor type is object/intersection/type-variable (upstream's flag test), which naturally recurses through the base's own static side; update collect_static_property_names (:3004) the same way so name enumeration agrees. Needs get_base_constructor_type_of_class from CLASS-GET-BASE-TYPES. Pitfall: the base constructor type is typeof C2 regardless of type arguments (they only matter for the instance side), so do not instantiate.
- **Finished alone (1):** conformance/classExtendingClass

### 106. `STRICT-SUBTYPE-FRESH-EMPTY-TARGET` (V; blocked 1, finished alone 1, lines 4; confidence medium)

- **Root cause:** Under subtype/strictSubtype relations a non-empty source is not related to a fresh empty object literal target; TSR treats `BarProps & object` as a strict subtype of fresh `{}` and keeps `{}`.
- **tsgo:** internal/checker/relater.go:3853 structuredTypeRelatedToWorker (fresh-empty-target arm; func at :3261)
- **TSR:** crates/tsr-checker/src/relater.rs:618 relate_ternary
- **Examples:**
  - `conformance/nonPrimitiveAndEmptyObject`: `fooProps`. want `BarProps & object`, got `{}`. Default `{}` in `const {fooProps = {}}` unions with BarProps & object under subtype reduction; upstream keeps the intersection (fresh {} target unrelated), TSR's relater relates the intersection source and reduction picks {} (classifier oracle-confirmed; not re-run).
- **Port plan:** Port the relater.go:3853 condition into relate_ternary for Relation::Subtype/StrictSubtype: target is empty object type with FreshLiteral and source not empty -> False; make sure intersection sources hit the same check (plain BarProps already works).
- **Finished alone (1):** conformance/nonPrimitiveAndEmptyObject

### 107. `IDENTIFIER-EXPORT-SYMBOL-OF-VALUE` (V; blocked 1, finished alone 1, lines 3; confidence medium)

- **Root cause:** checkIdentifier maps the resolved local through getExportSymbolOfValueSymbolIfExported before typing; TSR types the local directly, and get_type_of_symbol's ALIAS arm (87) wins over the EXPORT_VALUE arm (92) for an Alias|ExportValue local (an import merged with `export function Foo`).
- **tsgo:** internal/checker/checker.go:14383 getExportSymbolOfValueSymbolIfExported
- **TSR:** crates/tsr-checker/src/symbols.rs:87 get_type_of_symbol (ALIAS arm before EXPORT_VALUE marker arm 92)
- **Examples:**
  - `compiler/expandoFunctionContextualTypesNoValue`: `Foo`. want `{ (): void; bar: () => void; }`, got `any`. `import Foo from "blah"; export function Foo(){}; Foo.bar = ...`: the local Foo is Alias\|ExportValue. tsgo uses ExportSymbol (the function with expando); TSR takes the unresolved alias, giving any.
- **Port plan:** At identifier resolution sites (check_identifier and the related resolve_name callers with VALUE|EXPORT_VALUE meaning), apply export_symbol mapping when flags contain EXPORT_VALUE and export_symbol is set, as readonly_target.rs:179 already does. Then drop the EXPORT_VALUE marker arm in get_type_of_symbol if it becomes dead. Single witness.
- **Finished alone (1):** compiler/expandoFunctionContextualTypesNoValue

### 108. `CONDITIONAL-TAIL-RECURSION` (V; blocked 1, finished alone 1, lines 3; confidence high)

- **Root cause:** getConditionalType's tail-recursion loop (up to 1000 iterations without consuming instantiation depth) is not ported; TSR recurses and stops at ~60 levels.
- **tsgo:** internal/checker/checker.go:24300 getConditionalType (tailCount loop :24303-24310, getTailRecursionRoot :24418)
- **TSR:** crates/tsr-checker/src/declared.rs:6429 evaluate_conditional_alias / :6815 evaluate_conditional_node
- **Examples:**
  - `compiler/tailRecursiveConditionalTypes`: `T10`. want `"hello"`, got `error`. Trim<150 spaces + hello> is tail-recursive in the true branch; classifier probe shows TSR handles 60 levels but not 120.
- **Port plan:** Port the loop: when the chosen branch is a reference to a conditional alias (getTailRecursionRoot), replace root/mapper and continue the loop instead of recursing, with tailCount limit 1000 and the instantiation-depth counter untouched.
- **Finished alone (1):** compiler/tailRecursiveConditionalTypes

### 109. `JSDOC-PARAM-MATCH-BINDING-PATTERN` (V; blocked 1, finished alone 1, lines 3; confidence high)

- **Root cause:** findMatchingParameter matches a @param tag to a binding-pattern parameter by tag index; TSR's jsdoc_parameter_annotation returns None for any non-Identifier parameter name.
- **tsgo:** internal/parser/reparser.go:621 findMatchingParameter
- **TSR:** crates/tsr-checker/src/symbols.rs:5620 jsdoc_parameter_annotation (Identifier-only at :5630) and signatures.rs:1395 param_types name match
- **Examples:**
  - `conformance/optionalBindingParameters4`: `foo`. want `({ cause }?: { cause?: string; }) => string \| undefined`, got `({ cause }?: {}) => any`. `/** @param {{cause?: string}} [options] */ function foo({cause} = {})`: tag index 0 matches the pattern param 0. Probe same shape: `h : ({ c }?: {}) => any`.
- **Port plan:** Port findMatchingParameter: compute the tag's index among @param tags in its JSDoc; identifier params match by name (or by index when tag name empty), non-identifier params match by index. Use it in both jsdoc_parameter_annotation and the signatures.rs param_types collection (which is keyed by name).
- **Finished alone (1):** conformance/optionalBindingParameters4

### 110. `UNRESOLVED-IMPORT-ALIAS-VALUE-ERRORTYPE` (V; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause:** getTypeOfAlias for an import whose module is unresolvable yields errorType. With no error baseline (`// @ts-ignore` on the import) the types writer prints IsTypeAny types by intrinsic name `error`; TSR answers anyType for unfindable-module aliases, printing `any`.
- **tsgo:** internal/checker/checker.go:18598 getTypeOfAlias
- **TSR:** crates/tsr-checker/src/symbols.rs:409 get_type_of_alias (unfindable ES-import arm :465-479 inserts intrinsics.any)
- **Examples:**
  - `compiler/jsxLibraryManagedAttributesUnusedGeneric`: `React.createElement`. want `error`, got `any`. `// @ts-ignore import React from 'react'` (unresolved, so no .errors.txt baseline): React's value type is errorType, `typeof React.createElement` is errorType, and the writer prints `error`. TSR's §119 arm answers anyType, verdictdump 0:1/0:2 WRONG any.
- **Port plan:** Return intrinsics.error, not intrinsics.any, from get_type_of_alias's unfindable arms (symbols.rs:446-452 and :473-478), matching getTypeOfAlias. Make the types producer print `error` vs `any` for errorType from upstream's hadErrorBaseline rule (type_symbol_baseline.go:380-390), not from which intrinsic was chosen. Pitfall: these arms were calibrated (§31/§119) on error-baseline cases, where errorType prints `any` through the node builder. The change only matters when the producer distinguishes errorType from anyType, so confirm TSR's `error` gap sentinel prints `any` under error baselines before switching.
- **Finished alone (1):** compiler/jsxLibraryManagedAttributesUnusedGeneric

### 111. `EXTERNAL-MODULE-MEMBER-SHORTHAND-AMBIENT` (V; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause:** getExternalModuleMember returns the module symbol itself for a shorthand ambient module (`declare module "jquery";`); TSR lacks this arm and only special-cases direct imports from unfindable/shorthand modules elsewhere, so `export {x} from "jquery"` re-exports have no target.
- **tsgo:** internal/checker/checker.go:14693 getExternalModuleMember isShorthandAmbientModuleSymbol arm
- **TSR:** crates/tsr-checker/src/symbols.rs:1737 get_external_module_member
- **Examples:**
  - `conformance/ambientShorthand_reExport`: `x`. want `any`, got `error`. reExportUser imports x from ./reExportX, which does `export {x} from "jquery"`. The ExportSpecifier → get_external_module_member path has no shorthand arm (its doc calls it unreachable), while the direct-import path is covered by get_type_of_alias's unfindable special case and calls.rs:2350.
- **Port plan:** After resolve_external_module_name, if is_shorthand_ambient_module(module_symbol), return Some(module_symbol); get_type_of_symbol already answers any for such module symbols (symbols.rs:2740). Then audit the direct-import compensations (get_type_of_alias's ES-import unfindable arm, calls.rs:2320 block) for removal where they only covered shorthand modules.
- **Finished alone (1):** conformance/ambientShorthand_reExport

### 112. `MAPPED-KEYOF-MEMBER-ORDER` (V; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause:** Homomorphic mapped members follow getPropertiesOfType(modifiers) declaration order; TSR's property name enumeration for Number yields a different order.
- **tsgo:** internal/checker/checker.go:22727 forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType
- **TSR:** crates/tsr-checker/src/mapped.rs:481 mapped_member_keys (property_names_of at L529)
- **Examples:**
  - `conformance/mappedTypes1`: `x4`. want `{ toString: void; toFixed: void; toExponential: void; toPrecision: void; valueOf: void; toLocaleString: void; }`, got `{ valueOf: void; toPrecision: void; toLocaleString: void; toString: void; toFixed: void; toExponential: void; }`. f4<T1 extends Number>() infers T1=Number; probe m3.ts reproduces the TSR order, so property_names_of(Number) does not return merged-interface declaration order.
- **Port plan:** Make the homomorphic branch iterate get_properties_of_type(source) in symbol-table/declaration order (merged lib interface declarations in file order), matching getPropertiesOfType; check property_names_of for sorting or hash-order iteration.
- **Finished alone (1):** conformance/mappedTypes1

### 113. `OBJLIT-MISSING-NAME-PROPERTY-MEMBER` (V; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause:** A parser-recovered PropertyAssignment with a missing (empty) name still binds a property symbol named "" and checkObjectLiteral adds it to propertiesTable (`"": any`); TSR's check_object_literal skips every empty-text identifier name.
- **tsgo:** internal/checker/checker.go:13144 checkObjectLiteral (member added at :13331 regardless of empty name)
- **TSR:** crates/tsr-checker/src/objects.rs:857 check_object_literal (`PropertyName::Identifier(name) if name.text.is_empty() => continue` at :1511)
- **Examples:**
  - `conformance/objectLiteralShorthandPropertiesErrorWithModule`: `y`. want `{ m: typeof m; "": any; }`, got `{ m: typeof m; }`. `{ m.x }` recovers to shorthand `m` plus a PropertyAssignment with missing name and initializer `.x` (baseline records `> : any`, `>.x : any`); upstream keeps the "" member. TSR parses the same nodes (those lines are RIGHT) but drops the member at objects.rs:1511.
- **Port plan:** Remove the blanket empty-identifier `continue` and admit the member under name "" printed `""` (quoted, since not identifier text). Pitfall: the arm exists for templateStringInPropertyName1/2/ES6_* where tsgo prints the literal as `{}` — first establish (ast dump of both parsers) what node upstream has there; if tsgo's object literal has NO element in the template case, the TSR parser is producing an extra element and the right fix is in the parser, not a kind-based gate here. Medium confidence because that discriminator is unverified.
- **Finished alone (1):** conformance/objectLiteralShorthandPropertiesErrorWithModule

### 114. `LATE-BIND-MEMBER-OBJLIT-ACCESSOR-MERGE` (V; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause:** lateBindMember also runs for object-literal containers (SymbolFlagsLateBindingContainer includes ObjectLiteral), merging a computed `get`/`set` pair into one late symbol whose getTypeOfAccessors reads the getter; TSR's late_bound_members_of only walks class/interface/type-literal declarations, so the §523 pair reconstruction misses object-literal setters.
- **tsgo:** internal/checker/checker.go:16005 lateBindMember (via :15930 getResolvedMembersOrExportsOfSymbol for the object-literal symbol) -> :18511 getTypeOfAccessors
- **TSR:** crates/tsr-checker/src/members.rs:2539 late_bound_members_of (no ObjectLiteralExpression arm in the match at :2553), consumed by crates/tsr-checker/src/symbols.rs:287 get_type_of_accessors_worker
- **Examples:**
  - `conformance/symbolDeclarationEmit10`: `x`. want `string`, got `any`. `set [Symbol.isConcatSpreadable](x) { }` in an object literal shares a late symbol with the getter returning '' so x: string. TSR's setter symbol is its own __computed and late_bound_members_of(parent) finds no members for an ObjectLiteralExpression parent; probe shows the identical class pair works (`x : string`) while the object-literal pair gives `x : any`.
- **Port plan:** Add the ObjectLiteralExpression arm (its properties: PropertyAssignment/ShorthandPropertyAssignment/MethodDeclaration/Get/SetAccessorDeclaration with computed names) to late_bound_members_of, with static=false. Pitfall: the object-literal symbol must be the `parent` recorded on the accessor's binder symbol for the §523 path at symbols.rs:297-317 to reach it — verify the binder sets it; long-term both this and the overload cluster want a single lateBindMember-built table instead of sibling walks.
- **Finished alone (1):** conformance/symbolDeclarationEmit10

### 115. `SETTER-PARAM-FROM-GETTER-OBJLIT` (V; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause:** getTypeForVariableLikeDeclaration's set-accessor-parameter arm takes the paired getter's return type for any container; TSR's paired_get_accessor only scans class members, so a JS object-literal setter param is any.
- **tsgo:** internal/checker/checker.go:16718 getTypeForVariableLikeDeclaration (set-accessor parameter arm)
- **TSR:** crates/tsr-checker/src/symbols.rs:5586 paired_get_accessor
- **Examples:**
  - `compiler/accessorDeclarationEmitJs`: `v`. want `string`, got `any`. /a.js `{ get value(){return 'value'}, set value(v){} }`: tsgo gives v the getter's return type; paired_get_accessor returns None for ObjectLiteralExpression containers.
- **Port plan:** Extend paired_get_accessor to ObjectLiteralExpression properties (match GetAccessor members by name), or better resolve via the merged symbol's GetAccessor declaration like ast.GetDeclarationOfKind. Pitfall: TS object literals already work via another road; ensure only one road remains.
- **Finished alone (1):** compiler/accessorDeclarationEmitJs

### 116. `IMPORTED-ALIAS-REFERENCE-WRITTEN-ARGS` (V; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause:** getTypeFromTypeAliasReference's import/export arm is not ported: a generic alias referenced through an import specifier (outside an alias body) gets alias = the resolved target with aliasTypeArguments = the WRITTEN arguments, so a bare `Foo` prints `Foo`; TSR prints the default-filled `Foo<string>`.
- **tsgo:** internal/checker/checker.go:23617 getTypeFromTypeAliasReference (IsTypeReferenceType import/export alias arm, 23617-23627)
- **TSR:** crates/tsr-checker/src/declared.rs:1397 get_type_from_type_reference (alias_road branch calling get_instantiated_type_reference at :1598)
- **Examples:**
  - `compiler/fixCrashAliasLookupForDefauledImport`: `element (1:2)`. want `Foo`, got `Foo<string>`. usage.ts `import {Foo} from "./input"; function bar<T>(element: Foo)` with `export type Foo<T = string> = {}`: resolveTypeReferenceName(Alias) finds the import, so newAlias=Foo with written args [] -> `Foo`. Oracle probe: cross-file `declare const e1: Foo` tsgo `Foo`, TSR `Foo<string>`; `Foo<number>` matches in both.
- **Port plan:** In the alias_road branch (declared.rs:1564-1605), when the import resolves to a TYPE_ALIAS target and no enclosing alias claimed the instantiation (alias_symbol_for_type_node(node) is None), mint the instantiation with display = the written argument count (create_type_reference_with_display(symbol, arguments, Some(node.type_arguments.len())) already exists for the lib partial-arity case) instead of the full default-filled list. Only the import/export/re-export road does this; a same-file bare `Foo` correctly prints `Foo<string>` upstream. Keep the instantiation key distinct from the full-arity one (upstream's key includes the alias and its args).
- **Finished alone (1):** compiler/fixCrashAliasLookupForDefauledImport

### 117. `IMPORT-EQUALS-RHS-NAMESPACE-LOOKUP` (V; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause:** Identifiers in an import-equals RHS are typed via getSymbolAtLocation → getSymbolOfPartOfRightHandSideOfImportEquals, a namespace-exports lookup; an unresolved segment gives errorType (any). TSR's writer falls through to value-expression typing and finds Object.prototype.toString.
- **tsgo:** internal/checker/checker.go:14474 getSymbolOfPartOfRightHandSideOfImportEquals
- **TSR:** crates/tsr-conformance/src/types_producer.rs:1285 import-equals qualified-name leaf rule (falls through when exports lookup misses)
- **Examples:**
  - `compiler/importEqualsError45874`: `toString`. want `any`, got `() => string`. `import Foo = globals.toString.Blah`: toString is not in globals' exports, so upstream gives any. TSR's leaf rule at 1302 misses and later generic code types it as a property of the namespace value.
- **Port plan:** When the node sits in an import-equals RHS entity name, the leaf rule must be terminal: answer the exports-lookup result's type, or any when the lookup misses, never falling through to the expression path. Keep the declared-then-value order already there.
- **Finished alone (1):** compiler/importEqualsError45874

### 118. `PRINT-ARRAY-TARGET-DECLARED-TYPE` (V; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause:** The declared (self-referential) type of the global Array/ReadonlyArray interface is printed by typeReferenceToTypeNode as `T[]`/`readonly T[]`; TSR's declared-interface mint prints `Array<T>`.
- **tsgo:** internal/checker/nodebuilderimpl.go:2977 typeReferenceToTypeNode (globalArrayType/globalReadonlyArrayType arm 2979-2995)
- **TSR:** crates/tsr-checker/src/declared.rs:6291 new_named_type (via declared.rs:5949 get_declared_type_of_class_or_interface; the `T[]` spelling rule lives only in declared.rs:5371 type_reference_text)
- **Examples:**
  - `compiler/importExportInternalComments`: `Array (1:0)`. want `T[]`, got `Array<T>`. `export default Array;` types the identifier with Array's declared interface type, a TypeReference whose target is globalArrayType, which the node builder writes as `T[]`. TSR's new_named_type formats every generic class/interface declared type as `Name<Params>`.
- **Port plan:** When minting the declared type of the global Array (ReadonlyArray) interface, print it through the same rule type_reference_text applies to Array references (element text + `[]`, `readonly` prefix for ReadonlyArray) instead of `Name<Params>`; do it by routing the declared-type print through type_reference_text with the type parameters as arguments rather than adding a second spelling rule. Not related to alias attachment; unrelated to the other clusters in this group.
- **Finished alone (1):** compiler/importExportInternalComments

### 119. `SPECIFIER-PROPERTYNAME-IMMEDIATE-ALIAS` (V; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause:** getSymbolAtLocation on an export specifier's propertyName returns getImmediateAliasedSymbol (the target module's export), so the line types that export even when it is any; TSR's writer treats an any target as 'unresolved' and falls through to local scope lookup.
- **tsgo:** internal/checker/checker.go:2155 getImmediateAliasedSymbol (via getSymbolAtLocation specifier propertyName arm)
- **TSR:** crates/tsr-conformance/src/types_producer.rs:703 specifier property-name arm (any/error fallthrough 717-723)
- **Examples:**
  - `compiler/reexportNameAliasedAndHoisted`: `Sizing`. want `any`, got `typeof Sizing`. `export { Sizing as GridViewSizing } from './gridview'` where gridview `const Sizing = null` (any, strict off). The arm computes any and falls through, and the free-identifier road finds the local `namespace Sizing`.
- **Port plan:** Make the property-name arm terminal: return the target's type, any included, and turn error into the writer's any. The fallthrough exists only for globalThisGlobalExportAsGlobal, so land it together with GLOBALTHIS-SYMBOL-IN-GLOBALS. Upstream's symbol is the immediate aliased symbol (one hop), so use the specifier's resolve_alias target's type rather than the alias's own type when they differ.
- **Finished alone (1):** compiler/reexportNameAliasedAndHoisted

### 120. `CREATE-GENERATOR-TYPE-EMPTY-FALLBACK` (V; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause:** createGeneratorType returns emptyObjectType `{}` when neither global Generator nor IterableIterator exists; TSR's `?` on the missing global turns the signature into error.
- **tsgo:** internal/checker/checker.go:20434 createGeneratorType (fallback :20442-20447)
- **TSR:** crates/tsr-checker/src/signatures.rs:2497 return_type_from_body (generator global lookup 3078-3084)
- **Examples:**
  - `conformance/generatorReturnTypeFallback.2`: `f  (@lib: es5, function* f() { yield 1; })`. want `() => {}`, got `any (error)`. es5 lib has neither Generator nor IterableIterator, so upstream returns emptyObjectType after reporting. TSR's global_type_symbol_with_arity(...)? returns None for the whole body.
- **Port plan:** Extract create_generator_type(yield, return, next, is_async) mirroring createGeneratorType: resolveIterationType on yield/return (unknown fallback), Generator/AsyncGenerator, else IterableIterator/AsyncIterableIterator, else the empty object type (the `{}` intrinsic TSR already prints). Reuse it from getContextualTypeForYieldOperand's yield* arm and checkGeneratorInstantiationAssignabilityToReturnType.
- **Finished alone (1):** conformance/generatorReturnTypeFallback.2

### 121. `CONDITIONAL-INLINE-NODE-INSTANTIATION` (V; blocked 7, finished alone 0, lines 40; confidence high)

- **Root cause:** getConditionalTypeInstantiation for an anonymous conditional type node (signature return/param, interface member) is only ported for type-parameter constraints; elsewhere TSR returns error, and deferred results are also turned into error.
- **tsgo:** internal/checker/checker.go:22485 getConditionalTypeInstantiation
- **TSR:** crates/tsr-checker/src/declared.rs:6728 instantiate_conditional_node (L6759 `if !in_constraint return error`; L6776 deferred-result filter)
- **Examples:**
  - `conformance/templateLiteralTypes2`: `takesLiteral("foo.bar.baz")`. want `"baz"`, got `any`. Return type is an inline `T extends `foo.bar.${infer R}` ? R : unknown`; instantiation hits L6759 → error (classifier probe: same via alias works).
  - `compiler/booleanFilterAnyArray`: `anys.filter`. want `{ <S extends any>(cb1: ...): Ari<any>; ...}`, got `error`. Interface member return is an inline conditional on T; instantiating Ari<any> members goes through the same error return.
  - `conformance/inferTypes2`: `foo2(obj)`. want `T extends { x: infer P extends number ? infer P : string; } ? P : never`, got `error`. Instantiation with mapper T_1→T leaves it deferred; L6776 filters deferred results to error.
- **Port plan:** Remove the in_constraint restriction and the deferred-result filter: port getConditionalTypeInstantiation (mapper composition, distributive mapping over union check types, caching per root+mapper) so any conditional node instantiates, and when getConditionalType defers, produce a deferred ConditionalType carrying root+mapper that prints from instantiated parts. Needs the deferred-conditional printing to use the mapper (the comment at L6772 names exactly this missing node builder).
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (7):** compiler/booleanFilterAnyArray (+INFERENCE-GENERIC-SOURCE-SIG); compiler/contextualParamTypeVsNestedReturnTypeInference2 (+MERGED-FUNCTION-INTERFACE, unaligned); compiler/contextualParamTypeVsNestedReturnTypeInference3 (+MERGED-FUNCTION-INTERFACE, unaligned); compiler/contextualSignatureConditionalTypeInstantiationUsingDefault (+, unaligned); compiler/doYouNeedToChangeYourTargetLibraryES2015 (+ARRAY-LITERAL-TUPLE-CONTEXT, CHOOSE-OVERLOAD-CONTEXT-SENSITIVE-ARG-RETENTION, INFER-NO-CANDIDATE-GUARD, SIGNATURE-DECLARATION-ANNOTATION-REUSE); conformance/inferTypes2 (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT); conformance/templateLiteralTypes2 (+INFERENCE-PRIMITIVE-CONSTRAINT-REGULAR)

### 122. `CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES` (V; blocked 6, finished alone 0, lines 28; confidence high)

- **Root cause:** Inherited instance members come from resolveObjectTypeMembers merging getBaseTypes(source) (base TYPES, from the extends expression's construct signatures); TSR walks base SYMBOLS via base_symbols_of/base_symbol_of_heritage_entry/heritage_entity_symbol, which return None for any extends expression that is not an entity name of a class/interface (value variable, call, class expression, `(await import()).B`, intersection ctor), so the whole member lookup gaps.
- **tsgo:** internal/checker/checker.go:19106 resolveObjectTypeMembers (base merge :19127-19152) over :19167 getBaseTypes
- **TSR:** crates/tsr-checker/src/members.rs:3087 get_property_of_declared_symbol and :1675 generic_heritage_member (via :3182 base_symbols_of_ex / :3211 base_symbol_of_heritage_entry / :3238 heritage_entity_symbol)
- **Examples:**
  - `conformance/classExpression3`: `c.a`. want `number`, got `error`. `class extends class extends class { a = 1 } { b = 2 } { c = 3 }`: tsgo's base types come from getBaseConstructorTypeOfClass(check class expression) -> construct return (Anonymous class), whose members include a. TSR base_symbol_of_heritage_entry gets a ClassExpression expression, heritage_entity_symbol returns None, base_symbols_of returns None and c.a is a miss.
  - `conformance/mixinClassesMembers`: `this.a`. want `number`, got `error`. `class C2 extends Mixed1` with `const Mixed1: typeof M1 & typeof C1`: resolveBaseTypesOfClass takes the intersection's construct-signature return M1 & C1, so this.a resolves. TSR resolves `Mixed1` in TYPE meaning, finds a variable, gaps; probe h.ts `const x = A; class D1 extends x { m(){ return this.a } }` reproduces (TSR error, tsgo number).
- **Port plan:** After CLASS-GET-BASE-TYPES lands, make class/interface instance member resolution consume get_base_types: in get_property_of_declared_symbol and generic_heritage_member, for a CLASS owner iterate get_base_types(owner) (types, already instantiated with heritage args/defaults) and look the name up with get_property_of_type on each (instantiate_for_reference with the receiver as today), instead of base_symbols_of. Interfaces may keep their symbol path initially, but the class arm must not fall back to symbol walking when get_base_types answers. Other base_symbols_of callers (member_completeness.rs:264/387, nonexistent_property.rs:510, index_constraint.rs:319, flow.rs:1474/6916, declared.rs:7815/7911, members.rs:2481/2494/3004/3049) have the same blind spot and should migrate together or explicitly treat 'class with expression base' via get_base_types to avoid disagreeing completeness answers. Pitfall: base_symbols_of returning None also gates 'member set complete' decisions (excess-property / TS2339 diagnostics), so a half migration can flip diagnostics.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (6):** compiler/declarationEmitExpressionInExtends (+CLASS-GET-BASE-TYPES); conformance/classExpression3 (+CLASS-GET-BASE-TYPES); conformance/classExpressionES63 (+CLASS-GET-BASE-TYPES); conformance/importCallExpression3ES2020 (+CLASS-GET-BASE-TYPES); conformance/importCallExpressionInCJS4 (+CLASS-GET-BASE-TYPES); conformance/mixinClassesMembers (+CLASS-GET-BASE-TYPES, SUPER-FROM-BASE-TYPES)

### 123. `SUPER-FROM-BASE-TYPES` (V; blocked 4, finished alone 0, lines 13; confidence high)

- **Root cause:** checkSuperExpression answers errorType when getBaseTypes(class) is empty, getBaseConstructorTypeOfClass(class) for super(...)/static, else getTypeWithThisArgument(getBaseTypes[0]); TSR's check_super_expression re-derives the base from the extends entry's symbol (base_symbol_of_heritage_entry), so expression bases answer error, arity errors are only half detected, and a merged-interface base is ignored.
- **tsgo:** internal/checker/checker.go:7854 checkSuperExpression (base arm :7933-7961)
- **TSR:** crates/tsr-checker/src/expressions.rs:2007 check_super_expression (tail :2125-2165)
- **Examples:**
  - `compiler/missingPropertiesOfClassExpression`: `super`. want `typeof (Anonymous class)`, got `any`. `class George extends class { reset() {...} } { constructor() { super(); } }`: getBaseTypes(George) is the class expression instance, so super() answers getBaseConstructorTypeOfClass = typeof (Anonymous class). TSR's base_symbol_of_heritage_entry returns None for a ClassExpression and the super line is error/any.
  - `compiler/interfaceMergeWithNonGenericTypeArguments`: `super`. want `typeof SomeBaseClass`, got `any`. `interface MergedClass extends SomeInterface {}` + `class MergedClass extends SomeBaseClass<any>`: the class base errors on arity but resolveBaseTypesOfInterface adds SomeInterface, so getBaseTypes is non-empty and super() = getBaseConstructorTypeOfClass = typeof SomeBaseClass (heritage line = SomeInterface). TSR returns error because type_arguments.len() > type_parameter_count_of(base).
  - `compiler/superCallFromClassThatDerivesFromGenericTypeButWithIncorrectNumberOfTypeArguments1`: `super`. want `any`, got `typeof A`. `class B extends A<number>` with A<T1,T2>: too few arguments, getTypeFromClassOrInterfaceReference errors, getBaseTypes empty, super is errorType. TSR's arity check only tests 'too many' (documented residue at expressions.rs:2155) and answers typeof A.
- **Port plan:** Keep the container walk; replace the tail (expressions.rs:2125-2165) with upstream's: extends element absent -> error; classDeclarationExtendsNull -> null-widening/error; base = get_base_types(class)[0] else error; static or call -> get_base_constructor_type_of_class(class); else base with this-arg. Delete type_parameter_count_of and the instance_base_type_of_heritage_entry call here. mixinClassesMembers' this.f/f lines (C3.f(){ return super.f(); }) flip once super is M2 & M1 & C1. Pitfall: the existing 'object literal container -> error' decline (upstream any) is a separate deliberate choice; do not change it in this item.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (4):** compiler/interfaceMergeWithNonGenericTypeArguments (+CLASS-GET-BASE-TYPES); compiler/missingPropertiesOfClassExpression (+CLASS-GET-BASE-TYPES); compiler/superCallFromClassThatDerivesFromGenericTypeButWithIncorrectNumberOfTypeArguments1 (+CLASS-GET-BASE-TYPES, UNTYPED-CALL-ARGUMENT-ANY-CONTEXT); conformance/mixinClassesMembers (+CLASS-GET-BASE-TYPES, CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES)

### 124. `PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED` (V; blocked 4, finished alone 0, lines 7; confidence high)

- **Root cause:** Parameter type serialization: upstream prints getTypeOfSymbol(param), which includes the optional `| undefined` under strict. It also adds getOptionalType for required-initialized params (requiresAddingImplicitUndefined). The written annotation is reused only when it is equivalent modulo that undefined. TSR's Parameter carries the raw annotation type, so a param whose written text is dropped (an instantiated signature) or a required-initialized param never gets `| undefined`.
- **tsgo:** internal/checker/nodebuilderimpl.go:2181 serializeTypeForDeclaration (:2216-2220 requiresAddingUndefined/getOptionalType; :2249 isOptionalAnnotated equivalence; :2290 fallback typeToTypeNode(t)); emitresolver.go:581 requiresAddingImplicitUndefined / :625 isRequiredInitializedParameter
- **TSR:** crates/tsr-checker/src/signatures.rs:4670 parameter_of (r#type = raw annotation type, :4859) and crates/tsr-checker/src/inference.rs:5831 instantiate_signature (param loop :5862 drops written_text but keeps the undefined-less type)
- **Examples:**
  - `compiler/arrayFlatNoCrashInference`: `arr.flat`. want `<A, D extends number = 1>(this: A, depth?: D \| undefined) => FlatArray<A, D>[]`, got `<A, D extends number = 1>(this: A, depth?: D) => FlatArray<A, D>[]`. flat is reached through an instantiated Array<T> signature, so D is a fresh type parameter. The annotation `D` is then not equivalent and tsgo prints the symbol type `D \| undefined`. Oracle probe ra.ts `arr.o` gives `depth?: D \| undefined, t?: string \| undefined, u?: number` (the unchanged `u` still reuses); TSR drops every `\| undefined`.
  - `compiler/verbatim-declarations-parameters`: `foo1`. want `(..., resolveType: Map \| undefined, requiredParam: number) => void`, got `(..., resolveType: Map, requiredParam: number) => void`. `resolveType: Map = {}` is followed by a required param, so isRequiredInitializedParameter adds undefined. Oracle probe `function ri(a: string = "", b: number)` prints `(a: string \| undefined, b: number)`; TSR prints `a: string`.
  - `compiler/ipromise2`: `p.then`. want `<U>(success?: ((value: string) => Windows.Foundation.IPromise<U>) \| undefined, ..., progress?: (progress: any) => void)`, got `<U>(success?: (value: string) => Windows.Foundation.IPromise<U>, ...)`. On p: IPromise<string> the success/error params change type under T->string, so reuse fails and the symbol's optional type (with undefined) prints. `progress` is unchanged, so it reuses the written node, matching the probe's `u?: number`.
- **Port plan:** Give the printer upstream's serializeTypeForDeclaration result for each parameter. Let t = the symbol type, i.e. the annotation type with getOptionalType applied for `?` params under strictNullChecks (as get_type_of_symbol already does for the declaration line). If requiresAddingImplicitUndefined (strict && initializer && not optional && no undefined in the declared type), apply getOptionalType again. Reuse the written node when it is equivalent to t with undefined stripped (optional) or exactly; for required-initialized params, append `| undefined` to the reused node (:2264-2276). Otherwise print t. In TSR: keep a separate print type on Parameter, or compute it from the declaration at print time. instantiate_signature must instantiate that type too. When its written_text is cleared, the fallback renders the undefined-carrying type, not Parameter.r#type. Pitfall: the comment at signatures.rs:4842-4858 explains why r#type is raw (assertWeird: `(value?: string)`). That case stays correct because the written node is equivalent modulo undefined, so keep the reuse path first.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (4):** compiler/arrayFlatNoCrashInference [type-operators] (+CONDITIONAL-DEFERRAL-GATE); compiler/arrayFlatNoCrashInferenceDeclarations [type-operators] (+CONDITIONAL-DEFERRAL-GATE); compiler/ipromise2 (+CHOOSE-OVERLOAD-GENERIC-WALK, RENAMED-TYPE-PARAM-KEEPS-WRITTEN-NODE); compiler/verbatim-declarations-parameters [type-operators] (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT)

### 125. `REMOVE-SUBTYPES-UNDECIDABLE` (V; blocked 3, finished alone 0, lines 13; confidence medium)

- **Root cause:** removeSubtypes (UnionReductionSubtype from conditional expressions and ||) needs isTypeStrictSubtypeOf over concrete object pairs (tuple vs object literal, unique symbol vs object); TSR union_with_subtype_reduction returns None when its relater cannot decide a pair and the caller answers error.
- **tsgo:** internal/checker/checker.go:25934 removeSubtypes (callers: :10934 checkConditionalExpression, :12509 checkBinaryLikeExpressionWorker)
- **TSR:** crates/tsr-checker/src/unions.rs:1516 union_with_subtype_reduction
- **Examples:**
  - `conformance/literalTypes2`: `c8`. want `"hello" \| { kind: 123; } \| [1 \| 2, "bar" \| "foo"]`, got `error`. `cond ? c6 : cond ? c7 : 'hello'` with object literal c6 and tuple c7; neither is a subtype of the other, upstream keeps both. TSR probe `c ? {kind:1} : [1,'a'] as [number,string]` -> error.
  - `compiler/destructuringAssignmentWithDefault`: `options \|\| {}`. want `{} \| [string, number]`, got `error`. \|\| arm unions tuple with fresh {}; the fresh-empty-target rule keeps both. TSR probe `opt \|\| {}` -> error from union_with_subtype_reduction None.
- **Port plan:** Port removeSubtypes faithfully (hasObjectTypes, the 'every constituent identical-kind' fast path, isTypeStrictSubtypeOf with the fresh empty object literal target rule at relater.go:3853) and make the relater answer definitively for tuple/array vs object-literal and unique-symbol vs object pairs instead of returning undecided; then delete the Option/None path. Check generatorYieldContextualType 0:93 (unique symbol | QuickPickItem) as the third shape.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (3):** compiler/destructuringAssignmentWithDefault (+TUPLE-OPTIONAL-ELEMENT-UNDEFINED, UNION-PROP-OBJECT-LITERAL-MISSING); conformance/generatorYieldContextualType (+APPEND-LOCAL-TYPE-PARAMETERS, CONDITIONAL-BRANCH-NAMED-UNION, CONTEXTUAL-RETURN-GENERATOR-FILTER, GENERATOR-ANNOTATION-ITERATION-TYPES, INDEXED-ACCESS-GENERIC-DEFER, ORIGIN-ENTRY-ORDER, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE, unaligned); conformance/literalTypes2 (+BINDING-PATTERN-CONTEXT-LITERAL, INFERENCE-SUPERTYPE-NAMED-UNION, ORIGIN-SLICE-GATE)

### 126. `SPECIFIER-FOR-MODULE-SYMBOL` (V; blocked 3, finished alone 0, lines 6; confidence medium)

- **Root cause:** getSpecifierForModuleSymbol (module specifier generation) is only approximated: TSR spells `import("./stem")` from its stored path without removing declaration extensions and declines for any path with directories (node_modules packages).
- **tsgo:** internal/checker/nodebuilderimpl.go:1249 getSpecifierForModuleSymbol
- **TSR:** crates/tsr-checker/src/checker.rs:2806-2857 symbol_chain (file-module specifier arm)
- **Examples:**
  - `compiler/inferrenceInfiniteLoopWithSubtyping`: `addResolver`. want `... import("./graphql-compose").Thunk<...>`, got `... import("./graphql-compose.d").Thunk<...>`. Stem is the stored path with only .ts removed; tspath.RemoveFileExtension strips .d.ts.
  - `compiler/symbolLinkDeclarationEmitModuleNames`: `create`. want `<T extends import("@loopback/context").Constructor<any>>...`, got `<T extends Constructor<any>>...`. Constructor lives in a node_modules package not imported here; tsgo generates the package specifier, TSR declines because stem contains '/'.
- **Port plan:** Implement getSpecifierForModuleSymbol via a modulespecifiers port: relative path from importing file dir, extension removal (RemoveFileExtension incl. .d.ts), node_modules package-name resolution (package.json name/types). Replace the same-dir slice and `!stem.contains('/')` decline. symbolLink cases also need ALIAS-REATTACH.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (3):** compiler/inferrenceInfiniteLoopWithSubtyping (+INSTANTIATE-MAPPED-TYPE-ALIAS-DROP); compiler/symbolLinkDeclarationEmitModuleNames (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS); compiler/symbolLinkDeclarationEmitModuleNamesRootDir (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS)

### 127. `SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER` (V; blocked 3, finished alone 0, lines 5; confidence medium)

- **Root cause:** getSymbolChain's export= container handling (getWithAlternativeContainers / getAliasForSymbolInContainer) is not ported: a namespace that is a module's `export =` target is named by the module itself, and its members qualify through the export= namespace name.
- **tsgo:** internal/checker/symbolaccessibility.go:117 getWithAlternativeContainers / :342 getAliasForSymbolInContainer (from nodebuilderimpl.go:1087 getSymbolChain)
- **TSR:** crates/tsr-checker/src/checker.rs:2745 symbol_chain
- **Examples:**
  - `compiler/importDeclFromTypeNodeInJsSource`: `EventEmitter`. want `typeof import("events")`, got `typeof import("events").EventEmitter`. namespace EventEmitter is export= of ambient module "events"; getAliasForSymbolInContainer maps it to the module so the chain is just the module. TSR appends the namespace name.
  - `compiler/reactHOCSpreadprops`: `this.props`. want `Readonly<{ children?: React.ReactNode; }> & ...`, got `Readonly<{ children?: ReactNode; }> & ...`. ReactNode's parent namespace React is the export= of react; alternative-container route yields React.ReactNode, TSR prints bare (symbol_chain doc says getWithAlternativeContainers not ported).
- **Port plan:** Port getWithAlternativeContainers (file/module whose export= is the container) and getAliasForSymbolInContainer into symbol_chain's parent iteration as in getSymbolChain's loop over getContainersOfSymbol. Pitfall: medium confidence that both members need the same arm; reactReadonly/HOC also blocked by CLASS-BASE/QUALIFIED-TYPEREF-MINT.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (3):** compiler/importDeclFromTypeNodeInJsSource (+EXTERNAL-MODULE-MEMBER-EXPORT-EQUALS); compiler/reactHOCSpreadprops (+CLASS-GET-BASE-TYPES, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE); compiler/reactReadonlyHOCAssignabilityReal (+CLASS-GET-BASE-TYPES, JSX-ATTR-CONTEXTUAL-LITERAL, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE)

### 128. `JSDOC-GATHER-TYPE-PARAMETERS` (V; blocked 2, finished alone 0, lines 6; confidence high)

- **Root cause:** Reparser gatherTypeParameters collects type parameters from ALL @template tags of a comment and puts the tag's {constraint} on the first parameter, and hosts @template on class declarations; TSR takes only the first @template tag for typedefs, never copies the constraint, and ignores @template on classes.
- **tsgo:** internal/parser/reparser.go:293 gatherTypeParameters (constraint :312-320; class arm in reparseHosted JSDocTemplateTag case)
- **TSR:** crates/tsr-checker/src/declared.rs:7922 local_type_parameters_of (+ tsr-parser/src/jsdoc.rs:534 parse_template_tag constraint=None on TypeParameterDeclaration)
- **Examples:**
  - `compiler/contravariantOnlyInferenceFromAnnotatedFunctionJs`: `fns`. want `Funcs<A, B>`, got `any`. Typedef Funcs has two separate `@template A` / `@template {Record<string, unknown>} B` tags; TSR's local_type_parameters_of find_map takes only the first, so Funcs<A,B> has wrong arity -> any (probe: `@template A`+`@template B` typedef -> any; `@template A, B` works). Probe also: `@template {Record<string,unknown>} B` prints `<B>` with no constraint.
  - `compiler/jsFileMethodOverloads`: `Example`. want `Example<T>`, got `Example`. `/** @template T */ class Example`: reparser sets class TypeParameters; TSR local_type_parameters_of reads ClassDeclaration.type_parameters only (probe: `Ex : Ex`).
- **Port plan:** Replace the find_map in local_type_parameters_of with gatherTypeParameters semantics: concat type_parameters from every @template tag in the doc; for function hosts signatures.rs:1395 already extends across tags. Carry the tag constraint: either have parse_template_tag store the constraint into the first TypeParameterDeclaration (upstream clones it there) or make type_parameter_constraint (members.rs:943) look up the owning JSDocTemplateTag when the param's parent is a template tag (first param only). Add a class arm: for ClassDeclaration/ClassExpression in JS with no written type params, take gathered @template params from the class's JSDoc (affects declared type printing and `this.value: T`).
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (2):** compiler/contravariantOnlyInferenceFromAnnotatedFunctionJs (+JSDOC-ON-FUNCTION-EXPRESSION-HOSTING); compiler/jsFileMethodOverloads (+JSDOC-OVERLOAD-SIGNATURES)

### 129. `MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS` (V; blocked 2, finished alone 0, lines 6; confidence high)

- **Root cause:** instantiateMappedType's per-constituent arms are not ported: a homomorphic mapped type whose type variable maps to a still-generic T must stay a deferred mapped type, and a non-object (e.g. `object`) passes through unchanged; TSR always resolves/renders members and only passes primitives through.
- **tsgo:** internal/checker/checker.go:22535 instantiateMappedType (instantiateConstituent)
- **TSR:** crates/tsr-checker/src/mapped.rs:783 instantiate_mapped_type_worker (L800 primitive-only passthrough; L838-866 unconditional member resolution/render)
- **Examples:**
  - `conformance/dependentDestructuredVariablesFromNestedPatterns`: `promises`. want `{ -readonly [P in keyof T]: PromiseSettledResult<Awaited<T[P]>>; }`, got `{ [x: number]: PromiseSettledResult<Awaited<T[number]>>; at: ...`. Promise.allSettled(t) with T extends readonly unknown[] maps P-variable to generic T; tsgo keeps a generic mapped type. Probe m4.ts reproduces TSR's expansion against T's array constraint.
  - `compiler/jsFileImportPreservedWhenUsed`: `_.mapValues(obj, ...)`. want `object`, got `error`. T=object is not an object-literal-ish constituent → instantiateMappedType returns it unchanged. Probe m1.ts m3({} as object) → error.
- **Port plan:** Port instantiateMappedType/instantiateConstituent: map the type variable; if result is generic (isGenericMappedType after instantiation) return a fresh deferred mapped type without resolving members; for arrays/tuples use instantiateMappedArrayType/TupleType; for Object|Intersection|AnyOrUnknown|InstantiableNonPrimitive instantiate anonymously; otherwise return the constituent unchanged (covers `object`). Replace L800-804 and gate L838-866 on non-generic.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (2):** compiler/jsFileImportPreservedWhenUsed (+INDEXED-ACCESS-NEVER-INDEX, TYPE-ALIAS-ACCESSIBILITY-GATE); conformance/dependentDestructuredVariablesFromNestedPatterns (+APPARENT-MAPPED-ARRAY-CONSTRAINT, DESTRUCTURE-FLOW-DEPENDENT, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, UNATTRIBUTED)

### 130. `COMPARE-TYPES-ARMS` (V; blocked 2, finished alone 0, lines 2; confidence high)

- **Root cause:** TSR compare_types lacks two CompareTypes arms: the intersection arm (compareTypeLists of constituents) and compareTypeNames' alias-symbol name (getTypeNameSymbol returns t.alias.symbol first, so an aliased function type sorts by its alias name).
- **tsgo:** internal/checker/utilities.go:415 CompareTypes (intersection arm ~:504; compareTypeNames :589, getTypeNameSymbol :607)
- **TSR:** crates/tsr-checker/src/unions.rs:1416 compare_types and :1325 compare_type_names (named_symbol_name :390)
- **Examples:**
  - `conformance/mappedTypes4`: `obj`. want `(T & null) \| (T & object)`, got `(T & object) \| (T & null)`. typeof obj === 'object' narrows T to T&null \| T&object; lists [T,null] < [T,object] since Null 1<<3 < NonPrimitive 1<<17. TSR has no intersection arm, falls to type id (oracle-confirmed on ord.ts).
  - `compiler/contextualTypeCaching`: `minimizer`. want `(WebpackPluginFunction \| WebpackPluginInstance)[] \| undefined`, got `(WebpackPluginInstance \| WebpackPluginFunction)[] \| undefined`. Alias WebpackPluginFunction = function type; tsgo names it by alias symbol. TSR named_symbol_name only names Named types, the anonymous function type gets None and sorts last (oracle on ord.ts: tsgo `(WF \| WI)[]`, TSR `(WI \| WF)[]`).
- **Port plan:** In compare_types add a branch for two TypeData::Intersection values -> compare_type_lists(types). In compare_type_names, consult the type's alias symbol (wherever TSR records alias for anonymous/function types) first, mirroring getTypeNameSymbol; then the existing Named text fallback. Re-measure: the named_symbol_name comment documents a prior regression (callWithSpread4) when names came from the wrong symbol.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (2):** compiler/contextualTypeCaching (+CONTEXTUAL-PROPERTY-OWN-MEMBERS); conformance/mappedTypes4 (+FORIN-VARIABLE-INDEX-TYPE, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS)

### 131. `ARRAY-LITERAL-SUBTYPE-REDUCTION` (V; blocked 1, finished alone 0, lines 18; confidence high)

- **Root cause:** checkArrayLiteral always unions element types with UnionReductionSubtype; TSR check_array_literal_value only subtype-reduces when the union has more than one object constituent, so `[t, base]` (T extends Base) stays (T | Base)[].
- **tsgo:** internal/checker/checker.go:8021 checkArrayLiteral (getUnionTypeEx(elementTypes, UnionReductionSubtype) at :8096)
- **TSR:** crates/tsr-checker/src/array_literals.rs:750 check_array_literal_value (object_constituent_count gate at :1136)
- **Examples:**
  - `conformance/heterogeneousArrayLiterals`: `[t, base]`. want `Base[]`, got `(T \| Base)[]`. T extends Base is a strict subtype of Base and removeSubtypes drops it. TSR probe `function g<T extends {b:1}>(t:T){return [t, base]}` -> (T \| { b: 1; })[].
- **Port plan:** Drop the object_constituent_count>1 gate and always apply subtype reduction to array-literal element unions; requires the relater to decide type-parameter-vs-object (constraint) strict subtyping, shared with LOGICAL-OR-COALESCE-GENERIC-GATE. Watch the many passing array cases that currently rely on the gate to avoid undecided pairs.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/heterogeneousArrayLiterals (+WIDEN-NULLABLE-TO-ANY)

### 132. `MAPPED-TUPLE-TEMPLATE-INSTANTIATION` (V; blocked 1, finished alone 0, lines 8; confidence high)

- **Root cause:** instantiateMappedTupleType applies an arbitrary template per tuple element; TSR's alias sequence path only normalizes identity-like templates, leaving `Awaitified<[...]>` unresolved.
- **tsgo:** internal/checker/checker.go:22593 instantiateMappedTupleType
- **TSR:** crates/tsr-checker/src/mapped.rs:896 instantiate_mapped_sequence (gated by declared.rs:3769 is_normalized_mapped_sequence)
- **Examples:**
  - `conformance/mappedTypesArraysTuples`: `all(a, b)`. want `Promise<[number, number]>`, got `Promise<Awaitified<[number, Promise<number>]>>`. Awaitified<T> = {[P in keyof T]: __Awaited<T[P]>} over a tuple; probe m3.ts `Awaitified<[number, Promise<number>]>` stays unresolved, while an inline-conditional template on an anonymous signature (m2.ts aw) resolves.
- **Port plan:** Port instantiateMappedTupleType: for each element instantiate the template with P := numeric-literal index (via instantiateMappedTypeTemplate) and rebuild the tuple with modifiers; make the alias road call it for any template, not just identity-like ones (relax is_normalized_mapped_sequence).
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/mappedTypesArraysTuples (+REVERSE-MAPPED-TUPLE, TUPLE-OPTIONAL-ELEMENT-OPTIONALITY)

### 133. `JSDOC-OVERLOAD-SIGNATURES` (V; blocked 1, finished alone 0, lines 7; confidence medium)

- **Root cause:** Reparser turns each @overload JSDoc block preceding a method into an overload signature (implementation keeps its own JSDoc @param types); TSR models no @overload signatures and jsdoc_parameter_annotation declines entirely when any host doc has @overload.
- **tsgo:** internal/parser/reparser.go:134 reparseUnhosted (KindJSDocOverloadTag arm; :233)
- **TSR:** crates/tsr-checker/src/signatures.rs:1358 JS signature-from-JSDoc collection (no @overload arm) + crates/tsr-checker/src/symbols.rs:5660 jsdoc_parameter_annotation @overload decline
- **Examples:**
  - `compiler/jsFileMethodOverloads`: `transform`. want `{ <U>(fn: (y: T) => U): U; (): T; }`, got `<U>(fn: (y: T) => U) => U`. Two @overload blocks + impl doc `@param {(y: T) => unknown} [fn]`; tsgo makes 2 signatures and types impl fn from its own block. TSR collapses to one sig built from the first doc, and the @overload decline leaves impl `fn : any`.
- **Port plan:** Port reparseUnhosted's @overload arm: each JSDoc carrying @overload on a method/function becomes a signature declaration (its @template/@param/@this/@returns) appended to getSignaturesOfSymbol's list ahead of the implementation, implementation excluded from the call signatures. Then the implementation's params read only the LAST (non-overload) doc, so drop the blanket decline at symbols.rs:5660 in favour of skipping @overload docs. Re-check overloadTag1 (the decline's reason).
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** compiler/jsFileMethodOverloads (+JSDOC-GATHER-TYPE-PARAMETERS)

### 134. `CLASS-TYPE-BASE-TYPE-VARIABLE-INTERSECTION` (V; blocked 1, finished alone 0, lines 6; confidence medium)

- **Root cause:** getTypeOfFuncClassEnumModuleWorker intersects a class's static type with getBaseTypeVariableOfClass (base constructor type that is/contains a type variable), giving `{ new(...): (Anonymous class); prototype: ... } & TBase` for the mixin `class extends Base`; TSR's class arm mints a text `typeof X` anonymous type and anonymous_class_written_name deliberately declines (error) on a parameter-typed base.
- **tsgo:** internal/checker/checker.go:16912 getTypeOfFuncClassEnumModuleWorker (:16923-16927 class arm) / :16936 getBaseTypeVariableOfClass
- **TSR:** crates/tsr-checker/src/symbols.rs:2732 get_type_of_func_class_enum_module_worker (class arm :2783-2791, decline in anonymous_class_written_name :3071-3100)
- **Examples:**
  - `compiler/anonClassDeclarationEmitIsAnon`: `class extends Base {        timestamp = Date.now();    }`. want `{ new (...args: any[]): (Anonymous class); prototype: Timestamped.(Anonymous class); } & TBase`, got `error`. Inside `Timestamped<TBase extends Constructor>(Base: TBase)`, the class expression's base constructor type is the type parameter TBase, so the class's static type is intersected with TBase; the printer then expands the anonymous class side structurally. TSR's anonymous_class_written_name returns None on the Parameter-declared base and the type is error, so Timestamped returns any and `Timestamped(
- **Port plan:** In the class arm of get_type_of_func_class_enum_module_worker, compute get_base_constructor_type_of_class(class) (from CLASS-GET-BASE-TYPES); if it is a type variable, or an intersection containing one, return get_intersection_type([class static type, that type variable]); remove the syntactic Parameter decline at symbols.rs:3071-3100. The printer must render a class static type that is an intersection constituent structurally (`{ new (...args: any[]): (Anonymous class); prototype: Timestamped.(Anonymous class); }`), as typeToString does when the anonymous class type is not accessible by `typeof` name; that printing is likely the larger half. 1:1/1:10 additionally need the `import("./wrapClass").Constructor` qualification of the constraint across files; 1:13 (super() = Timestamped(User) type) also needs SUPER-FROM-BASE-TYPES.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** compiler/anonClassDeclarationEmitIsAnon (+CLASS-GET-BASE-TYPES)

### 135. `GETTER-RETURN-ANNOTATION-CONTEXT` (V; blocked 1, finished alone 0, lines 6; confidence high)

- **Root cause:** getReturnTypeFromAnnotation's get-accessor arms (own return annotation, else paired setter's annotated param type) provide the contextual type of getter return expressions; TSR's ReturnStatement arm reads annotations only for function/arrow/method nodes, so getter returns get no context.
- **tsgo:** internal/checker/checker.go:20058 getReturnTypeFromAnnotation (getter arm :20066)
- **TSR:** crates/tsr-checker/src/contextual.rs:1417 get_contextual_type ReturnStatement arm (annotation match omits GetAccessor)
- **Examples:**
  - `conformance/objectLiteralGettersAndSetters`: `(t) => { var p: string; var p = t; }`. want `(t: string) => void`, got `error`. `get n() { return (t)=>{..} }` paired with `set n(x: (t: string) => void)`: contextual return = setter param annotation; TSR misses it (probe; even `get m(): (t:string)=>void { return (t)=>{} }` gives error).
- **Port plan:** Add GetAccessor to the annotation match at contextual.rs:1417 (own r#type), and when absent and the name is bindable, use the paired SetAccessor's parameter annotation (getAnnotatedAccessorType). Covers classes and object literals. Shares the declaration_takes_no_contextual_return GetAccessor arm with FUNCEXPR-NIL-CONTEXTUAL-SIGNATURE-POSITIONS (that one handles the no-annotation case).
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/objectLiteralGettersAndSetters (+OBJECT-LITERAL-PSEUDOTYPE-REUSE, OBJLIT-ACCESSOR-NONIDENT-NAME)

### 136. `KEYOF-ANY-TYPE-NODE` (V; blocked 1, finished alone 0, lines 4; confidence medium)

- **Root cause:** `keyof any` written as a type node yields error in TSR; getIndexType(any) is string | number | symbol.
- **tsgo:** internal/checker/checker.go:26684 getIndexTypeEx (any/never arm :26701)
- **TSR:** crates/tsr-checker/src/declared.rs:621 get_type_from_type_node TypeOperatorNode keyof arm
- **Examples:**
  - `compiler/genericCallInferenceInConditionalTypes1`: `Result1`. want `Omit<any, "ref"> & { ref?: Ref<HTMLElement> \| undefined; }`, got `error`. PropsWithoutRef<any> evaluates nested `"ref" extends keyof P` with P=any. Probe c9.ts: `type K = keyof any` → error, N<any> → error, N<{ref:1}> → 1, so the nested branch fails on keyof any, not on branch instantiation.
- **Port plan:** In the keyof TypeOperatorNode arm, resolve non-reference operands (keywords any/never/unknown, etc.) through resolved_keyof_type, whose worker already has the any|never arm (declared.rs:7634). Then re-check the nested-conditional cases.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** compiler/genericCallInferenceInConditionalTypes1 [type-operators] (+CONDITIONAL-TYPE-ROOT-ALIAS)

### 137. `RENAMED-TYPE-PARAM-KEEPS-WRITTEN-NODE` (V; blocked 1, finished alone 0, lines 4; confidence medium)

- **Root cause:** Upstream renames a shadowed signature type parameter (U -> U_1) as display-only remapping inside node building; reuse of the written return node survives the rename. TSR implements the rename by instantiate_signature with a fresh minted parameter, which clears written_return/written_text because the type id changed.
- **tsgo:** internal/checker/nodebuilderimpl.go:2023 serializeReturnTypeForSignature (reuse via pseudoTypeToNodeWithCheckerFallback -> tryReuseExistingNodeHelper with type-parameter remapping)
- **TSR:** crates/tsr-checker/src/inference.rs:5583 rename_type_parameters_for_site (calls instantiate_signature :5676, which clears written_return at :5874)
- **Examples:**
  - `compiler/ipromise2`: `then`. want `{ <U>(...): Windows.Foundation.IPromise<U>; <U_1>(...): Windows.Foundation.IPromise<U_1>; ... }`, got `{ <U>(...): Windows.Foundation.IPromise<U>; <U_1>(...): IPromise<U_1>; ... }`. Inside the first `then` overload, U is in scope, so the other overloads print U_1. tsgo still reuses their written return `Windows.Foundation.IPromise<U>` with U remapped. TSR's rename goes through instantiate_signature, which clears written_return, so it prints the shortest name `IPromise<U_1>`; probe ip.ts reproduces this.
- **Port plan:** Make the site rename display-only. Have rename_type_parameters_for_site keep written_text, written_return and written_constraint, applying the same textual U->U_1 substitution to them, or pass the rename map to the printer the way signature_to_string_at's apply_renames path does (signatures.rs:6972). Do not treat it as a real instantiation. Pitfall: the substitution must be token-aware, not substring-based (U inside `UType`). Prefer printing from the reused node with an identifier remap.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** compiler/ipromise2 (+CHOOSE-OVERLOAD-GENERIC-WALK, PARAM-SERIALIZED-TYPE-IMPLICIT-UNDEFINED)

### 138. `ORIGIN-ENTRY-ORDER` (V; blocked 1, finished alone 0, lines 3; confidence high)

- **Root cause:** getUnionTypeWorker builds the denormalized origin by insertType (CompareTypes on the entries themselves: a named union sorts by its Union flag bit, after type params/conditionals); TSR sorts origin entries by their first MEMBER's sort bits (enum literal) with nullables forced last.
- **tsgo:** internal/checker/checker.go:25653 getUnionTypeWorker (origin arm :25705-25728, insertType :26621)
- **TSR:** crates/tsr-checker/src/unions.rs:570 union_type_worker (entry key at :618-636)
- **Examples:**
  - `conformance/generatorYieldContextualType`: `value`. want `T \| Directive`, got `Directive \| T`. `value: Directive \| T` (enum Directive) gets origin [T, Directive]; T (TypeParameter 1<<19) < enum union (Union 1<<27). Oracle on ord.ts: tsgo `T \| D`, TSR `D \| T`; also `C<T> \| D` vs TSR `D \| C<T>`.
- **Port plan:** Replace the custom (nullable-last, first-member flags) key with plain compare_types on the entries, where compare_types uses sort_order_flags of the entry type itself (union flags for named unions). Pitfalls: the comment cites `MyEnum | undefined` and `boolean | E` baselines — re-check those under the plain comparator (boolean's Boolean bit already sorts before enum unions; the undefined-last spelling may come from the nodebuilder, not the origin) before deleting.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/generatorYieldContextualType (+APPEND-LOCAL-TYPE-PARAMETERS, CONDITIONAL-BRANCH-NAMED-UNION, CONTEXTUAL-RETURN-GENERATOR-FILTER, GENERATOR-ANNOTATION-ITERATION-TYPES, INDEXED-ACCESS-GENERIC-DEFER, REMOVE-SUBTYPES-UNDECIDABLE, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE, unaligned)

### 139. `JS-LITERAL-PROPERTY-ANY` (V; blocked 1, finished alone 0, lines 2; confidence high)

- **Root cause:** Property access on a JS object-literal type (ObjectFlagsJSLiteral) with no such property and no index info returns any (unless unchecked-JS suggestion); TSR applies js_literal_types only to element access, so property access errors.
- **tsgo:** internal/checker/checker.go:11334 checkPropertyAccessExpressionOrQualifiedName (isJSLiteralType arm; utilities.go:1753 isJSLiteralType)
- **TSR:** crates/tsr-checker/src/members.rs:73 check_property_access_expression
- **Examples:**
  - `compiler/amdLikeInputDeclarationEmit`: `module.exports`. want `any`, got `error`. `const module = {}; module.exports = X; return module.exports;` in JS: `module` stays `{}` (baseline line 61) and the missing property reads any via isJSLiteralType. Probe `const m = {}; m.exports;` in .js -> TSR `m.exports : error`.
- **Port plan:** In check_property_access_expression's missing-property path (after index-info lookup fails), return any when !no_implicit_any-independent upstream condition holds: !isUncheckedJSSuggestion && js_literal_types.contains(left_type) — mirror indexed.rs:637 but note upstream's property-access arm does not test noImplicitAny (isJSLiteralType itself does: utilities.go:1753 checks noImplicitAny first).
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** compiler/amdLikeInputDeclarationEmit (+IMPORT-TYPE-NODE-TYPEOF, RETURN-LITERAL-OWN-SIG-CONTEXT)

### 140. `CONDITIONAL-BRANCH-NAMED-UNION` (V; blocked 1, finished alone 0, lines 2; confidence medium)

- **Root cause:** When the selected (infer) branch is a union containing a NAMED union (multi-member enum or union alias), TSR's branch instantiation declines and keeps the alias, tsgo instantiates the branch normally.
- **tsgo:** internal/checker/checker.go:24427 getConditionalType instantiateType(trueType, trueMapper)
- **TSR:** crates/tsr-checker/src/declared.rs:7252 evaluate_conditional_inference
- **Examples:**
  - `conformance/generatorYieldContextualType`: `selection`. want `QuickPickItem[] \| Directive`, got `StepSelection<QuickPickStep<QuickPickItem>>`. Probe c7.ts: `T extends Q<infer U> ? U[] \| D : never` with `enum D {A, B}` or `type DD = 1\|2` stays alias; with single-member enum or inline `1\|2` it resolves.
- **Port plan:** Find where evaluate_conditional_inference instantiates/unions the true branch and make named unions (enum unions, alias unions) flow through get_union_type with origin like any other constituent; do not decline on Named union constituents.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/generatorYieldContextualType (+APPEND-LOCAL-TYPE-PARAMETERS, CONTEXTUAL-RETURN-GENERATOR-FILTER, GENERATOR-ANNOTATION-ITERATION-TYPES, INDEXED-ACCESS-GENERIC-DEFER, ORIGIN-ENTRY-ORDER, REMOVE-SUBTYPES-UNDECIDABLE, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE, unaligned)

### 141. `IMPORT-CALL-SYNTHETIC-DEFAULT-TYPE` (V; blocked 1, finished alone 0, lines 2; confidence medium)

- **Root cause:** checkImportCallExpression wraps the resolved module type with getTypeWithSyntheticDefaultOnly/getTypeWithSyntheticDefaultImportType; TSR's check_import_call_expression mints a bare `typeof import("m")` with no synthetic default.
- **tsgo:** internal/checker/checker.go:8267 checkImportCallExpression (:8305-8311)
- **TSR:** crates/tsr-checker/src/calls.rs:422 check_import_call_expression
- **Examples:**
  - `conformance/intersectionsAndEmptyObjects`: `import('./ex')`. want `Promise<{ default: typeof import("./ex"); }>`, got `Promise<typeof import("./ex.d")>`. ex.d.ts is `export {}` under commonjs+esModuleInterop; canHaveSyntheticDefault (decl-file arm) true so tsgo spreads `{default: module}`; TSR returns the unwrapped namespace (probe p6.ts reproduces). The `"./ex.d"` spelling is a second, independent defect: checker.rs type_to_string_at_worker's relative-file arm (:2167-2176) prints the module symbol name with only `.ts` stripped.
- **Port plan:** After computing the module, call the shared getTypeWithSyntheticDefaultImportType port (see ESM-NAMESPACE-SYNTHETIC-DEFAULT-WRAPPER) on the type of resolve_external_module_symbol(module) and wrap in Promise. Separately fix the .d.ts specifier spelling (strip `.d` per getSpecifierForModuleSymbol/module specifier ending rules) or the line stays WRONG. Line [0:100] `mock(import('./ex')) : {}` is downstream of the wrapped type (inference of M from Promise<{default}>); confirm after port.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/intersectionsAndEmptyObjects (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION)

### 142. `INFERENCE-SUPERTYPE-NAMED-UNION` (V; blocked 1, finished alone 0, lines 2; confidence medium)

- **Root cause:** getCommonSupertype's literal path calls the real getUnionType, whose named-union arm returns the existing alias union (Bit) when the candidates are exactly its members; TSR covariant_combination uses get_union_type_unprinted, skipping that arm.
- **tsgo:** internal/checker/inference.go:1530 getCommonSupertype -> checker.go:25653 getUnionTypeWorker (namedUnions[0] return)
- **TSR:** crates/tsr-checker/src/inference.rs:7160 covariant_combination (get_union_type_unprinted at :7189/:7198)
- **Examples:**
  - `conformance/literalTypes2`: `append(aa, 1)`. want `Bit[]`, got `(0 \| 1)[]`. Candidates Bit and 1 union to Bit's member set; upstream returns Bit. TSR probe gives (0 \| 1)[].
- **Port plan:** Use the printing get_union_type (with named-union/origin handling) in covariant_combination, or port the `alias == nil && len(namedUnions)==1 && reduced empty -> namedUnions[0]` check into the unprinted path. Verify unprinted was chosen to avoid the origin gate errors; after ORIGIN-SLICE-GATE lands this is safe.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/literalTypes2 (+BINDING-PATTERN-CONTEXT-LITERAL, ORIGIN-SLICE-GATE, REMOVE-SUBTYPES-UNDECIDABLE)

### 143. `MAPPED-AS-CLAUSE-INDEX-TYPE` (V; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause:** getIndexTypeForMappedType for an as-clause mapped type maps each key through the instantiated name type; TSR's keyof leaves the name-type conditional uninstantiated per key.
- **tsgo:** internal/checker/checker.go:26871 getIndexTypeForMappedType
- **TSR:** crates/tsr-checker/src/mapped.rs:461 mapped_index_type
- **Examples:**
  - `compiler/deferredConditionalTypes`: `FilteredRes1`. want `true`, got `false`. FilterByStringValue<[[]]> has as-clause `Equals<O[K],string> extends true ? K : never`; probe m3.ts `keyof F<[[]]>` prints the raw conditional `... ? K : never` repeated per key instead of never, so Values/Equals answer false.
- **Port plan:** In mapped_index_type instantiate name_type with the mapped type parameter := each key (instantiateType(nameType, appendTypeMapping(mapper, typeParameter, key))) and union the results, as getIndexTypeForMappedType does. Only line 0:39 of this case; 0:7-9 are in CONDITIONAL-DEFERRAL-GATE.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** compiler/deferredConditionalTypes (+CONDITIONAL-DEFERRAL-GATE, unaligned)

### 144. `MAPPED-TEMPLATE-ADD-OPTIONALITY-INDEX` (V; blocked 1, finished alone 0, lines 1; confidence high)

- **Root cause:** Index infos produced by resolveMappedTypeMembers use getTemplateTypeFromMappedType (addOptionality for `?`), TSR builds them from the raw template.
- **tsgo:** internal/checker/checker.go:22697 getTemplateTypeFromMappedType
- **TSR:** crates/tsr-checker/src/mapped.rs:595 resolve_mapped_type_members_worker
- **Examples:**
  - `conformance/mappedTypeModifiers`: `x["other"]`. want `number \| undefined`, got `number`. Partial<Foo> over Foo's string index; tsgo index value = addOptionality(template). Probe m1.ts pa["x"] on Partial<{[x:string]:number}> gives number.
- **Port plan:** Compute index-info value types from a ported getTemplateTypeFromMappedType (addOptionality when the modifiers include `?`) inside resolve_mapped_type_members_worker. Lines 130/132/137 of this case are in MAPPED-TYPE-TEXT-MINT-PRINT.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/mappedTypeModifiers [type-operators] (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT)

### 145. `TRYSYMBOLTABLE-UMD-ALIAS-EXCLUSION` (V; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause:** trySymbolTable excludes UMD `export as namespace` aliases when the enclosing file is a module; TSR's module_alias_at lacks that exclusion (best_name has it), so the module prints `typeof Foo` instead of falling back to import("./foo").
- **tsgo:** internal/checker/symbolaccessibility.go:566 trySymbolTable (isUMDExportSymbol arm)
- **TSR:** crates/tsr-checker/src/checker.rs:3380 module_alias_at
- **Examples:**
  - `conformance/umd5`: `Foo`. want `typeof import("./foo")`, got `typeof Foo`. `export as namespace Foo` in foo.ts (a module): UMD alias not usable there, chain falls to specifier; TSR picks the UMD alias.
- **Port plan:** Add the isUMDExportSymbol && IsExternalModule(enclosing file) exclusion to module_alias_at (reuse best_name's check at checker.rs:3637); ideally merge module_alias_at into the unified trySymbolTable walk of SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS. Single case; not oracle-probed.
- **Finished alone (0):** none
- **Also blocked (other clusters needed) (1):** conformance/umd5 (+QUALIFIED-NAME-LEFT-ALIAS-RESOLVE)

## Long-tail cluster details (wave-1 only)

### T1. `RECURSION-DEPTH` (1; blocked 3, finished alone 3, lines 73; confidence low)

- **Root cause / port plan:** Instantiation of a recursive anonymous return type (`function foo<T>() { var z = foo<typeof y>(); var y: { y2: typeof z }; return y }`) proceeds until instantiationDepth / circularity yields any at depth ~11 in tsgo (or `b: any` for a self-referential member); TSR stops after two levels with `{}`
- **tsgo:** internal/checker/checker.go:22100 instantiateType (instantiationDepth==100 / getTypeOfSymbol circularity -> any)
- **TSR:** crates/tsr-checker/src (instantiate_type depth / circular var type -> `{}`)
- **Example:** `compiler/cyclicGenericTypeInstantiation` [0:0] `foo`. want `<T>() => { y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: any; }; }; }; }; }; }; }; }; }; }; }`, got `<T>() => { y2: { y2: {}; }; }`
- **Example:** `compiler/cyclicGenericTypeInstantiation` [0:1] `z`. want `{ y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: { y2: any; }; }; }; }; }; }; }; }; }; }; }`, got `{ y2: { y2: {}; }; }`
- **Wave-1 evidence:** compiler/cyclicGenericTypeInstantiation: want 11-deep `{ y2: ... any }` got `{ y2: { y2: {}; }; }`; cyclicTypeInstantiation want `{ a: T; b: any; }`
- **Cases:** compiler/cyclicGenericTypeInstantiation; compiler/cyclicGenericTypeInstantiationInference; compiler/cyclicTypeInstantiation

### T2. `CONTEXTUAL-GENERIC-CONDITIONAL-CONSTRAINT` (1; blocked 3, finished alone 3, lines 40; confidence medium)

- **Root cause / port plan:** When the contextual type of an argument is a generic (deferred) conditional type over an inferring type parameter, getApparentTypeOfContextualType maps it through getApparentType -> getBaseConstraintOfType (conditional constraint: distributive/default constraint), yielding Props so `value => false` gets `value: string`; TSR yields no contextual signature (arrow answers error, param any)
- **tsgo:** internal/checker/checker.go:30686 getApparentTypeOfContextualType (getApparentType -> computeBaseConstraint conditional arm / getDefaultConstraintOfConditionalType)
- **TSR:** crates/tsr-checker/src/contextual.rs apparent_contextual_type (~L591) -> base_constraint_of_type / constraints.rs default_constraint_of_conditional_type
- **Example:** `compiler/conditionalTypeContextualTypeSimplificationsSuceeds` [0:10] `{ when: value => false }`. want `{ when: (value: string) => false; }`, got `error`
- **Example:** `compiler/conditionalTypeContextualTypeSimplificationsSuceeds` [0:11] `when`. want `(value: string) => false`, got `error`
- **Wave-1 evidence:** compiler/conditionalTypeContextualTypeSimplificationsSuceeds: probe `good3<P extends Props>(attrs: P extends Props ? P : P)` -> `value : any`, while `attrs: P` gives string
- **Cases:** compiler/conditionalTypeContextualTypeSimplificationsSuceeds; compiler/contextualTypeSelfReferencing; compiler/mappedTypeRecursiveInference2

### T3. `TAGGED-TEMPLATE-EFFECTIVE-ARGS` (1; blocked 3, finished alone 3, lines 6; confidence high)

- **Root cause / port plan:** Tagged templates resolve through the ordinary resolveCall with getEffectiveCallArguments = [synthetic TemplateStringsArray expression, ...span expressions] and the tag's this-argument. TSR's check_tagged_template_expression is a separate road that drops the strings parameter (shifted signatures) and never infers from the this-argument, so overload choice and generic this-inference diverge.
- **tsgo:** internal/checker/checker.go:30042 getEffectiveCallArguments (tagged-template arm) -> checker.go:8843 resolveCall
- **TSR:** crates/tsr-checker/src/calls.rs check_tagged_template_expression (~L1162, shifted overload pick at ~L1250)
- **Example:** `conformance/taggedTemplateStringsWithOverloadResolution2` [0:28] `c`. want `string`, got `number`
- **Example:** `conformance/taggedTemplateStringsWithOverloadResolution2` [0:29] `foo2 `${1}``. want `string`, got `number`
- **Wave-1 evidence:** taggedTemplateStringsWithOverloadResolution2: foo2`${1}` want string (TemplateStringsArray overload) got number; thisTypeInTaggedTemplateCall: Foo.m`test` want Foo got error while Foo.m("x") is Foo (p18.ts)
- **Cases:** conformance/taggedTemplateStringsWithOverloadResolution2; conformance/taggedTemplateStringsWithOverloadResolution2_ES6; conformance/thisTypeInTaggedTemplateCall

### T4. `IMPORT-ATTRIBUTES-ALIAS-GATE` (1; blocked 3, finished alone 2, lines 3; confidence high)

- **Root cause / port plan:** Default-import alias target (getTargetOfImportClause -> getTargetOfModuleDefault) is resolved regardless of an import attributes clause. TSR import_clause_default_target refuses any ImportDeclaration with `attributes` (a deliberate §292 decline), so the alias answers any.
- **tsgo:** internal/checker/checker.go:14528 getTargetOfImportClause / checker.go:14536 getTargetOfModuleDefault
- **TSR:** crates/tsr-checker/src/symbols.rs import_clause_default_target (~L1372 `import.attributes.is_none()`; also ~L1619, ~L1674)
- **Example:** `conformance/importAttributes7` [1:0] `a`. want `{ a: string; b: string; 1: string; }`, got `any`
- **Example:** `conformance/importAttributes8` [1:0] `a`. want `{ a: string; b: string; }`, got `any`
- **Wave-1 evidence:** conformance/importAttributes8: `import a from "./a" with {..}` a want { a: string; b: string; } / got any; same import without attributes types correctly
- **Cases:** conformance/importAttributes11; conformance/importAttributes7; conformance/importAttributes8

### T5. `CONTEXTUAL-BINDING-PATTERN-INITIALIZER` (1; blocked 2, finished alone 2, lines 26; confidence high)

- **Root cause / port plan:** initializer of an unannotated variable with a binding-pattern name is contextually typed by getTypeFromBindingPattern(includePatternInType); feeds hasContextualTypeWithNoGenericTypes constraint substitution and inference
- **tsgo:** internal/checker/checker.go:29423 getContextualTypeForInitializerExpression
- **TSR:** crates/tsr-checker/src/contextual.rs get_contextual_type VariableDeclaration arm (annotation only)
- **Example:** `compiler/narrowingDestructuring` [0:15] `a`. want `string`, got `error`
- **Example:** `compiler/narrowingDestructuring` [0:16] `value`. want `{ kind: "a"; a: string; }`, got `T`
- **Wave-1 evidence:** narrowingDestructuring: `const {a} = value` value want { kind: "a"; a: string; } / got T (annotated `const v: X = value` works)
- **Cases:** compiler/narrowingDestructuring; compiler/objectBindingPatternContextuallyTypesArgument

### T6. `OBJLIT-ACCESSOR-DEFERRED` (1; blocked 2, finished alone 2, lines 24; confidence high)

- **Root cause / port plan:** checkObjectLiteral only checkNodeDeferred's get/set accessors; their types resolve lazily, so a getter returning the variable itself is not circular
- **tsgo:** internal/checker/checker.go:13144 checkObjectLiteral (accessor arm :13314)
- **TSR:** crates/tsr-checker/src/objects.rs object literal accessor member typing (eager)
- **Example:** `compiler/noCircularitySelfReferentialGetter3` [0:0] `a`. want `{ prop: number; readonly self: any; }`, got `any`
- **Example:** `compiler/noCircularitySelfReferentialGetter3` [0:1] `{  prop: 42,  get self() {    return a;  },} satisfies { prop: number; self: any }`. want `{ prop: number; readonly self: { prop: number; readonly self: any; }; }`, got `error`
- **Wave-1 evidence:** noCircularitySelfReferentialGetter3: a want { prop: number; readonly self: any; } / got any; oracle: same without satisfies
- **Cases:** compiler/noCircularitySelfReferentialGetter3; compiler/noCircularitySelfReferentialGetter4

### T7. `VARIANCE-CIRCULARITY` (1; blocked 2, finished alone 2, lines 14; confidence low)

- **Root cause / port plan:** Property initializer `callme(this).num` with overloaded callme over the class's own generic type resolves in tsgo (no circularity); TSR reports self-referential circularity. Not minimised.
- **tsgo:** internal/checker/checker.go (getVariances / resolveCall on this-typed arg)
- **TSR:** crates/tsr-checker/src/symbols.rs property initializer circularity (resolutions stack)
- **Example:** `compiler/classVarianceResolveCircularity1` [0:2] `Value`. want `number`, got `any`
- **Example:** `compiler/classVarianceResolveCircularity2` [0:10] `bar`. want `Bar<any>`, got `any`
- **Wave-1 evidence:** classVarianceResolveCircularity1 `Value` want number got any
- **Cases:** compiler/classVarianceResolveCircularity1; compiler/classVarianceResolveCircularity2

### T8. `PLUS-ASSIGNABLE-TO-KIND` (1; blocked 2, finished alone 2, lines 12; confidence high)

- **Root cause / port plan:** `+` operator classification: checkBinaryLikeExpression asks isTypeAssignableToKindEx(operand, NumberLike, true), i.e. full assignability to `number` (for T[K] via getConstraintFromIndexedAccess -> Record<K, number>[K] = number). TSR's check_addition tests raw flags (+ one level of union, + bare type-parameter constraint) so a generic indexed-access operand falls to the error tail.
- **tsgo:** internal/checker/checker.go:12336 checkBinaryLikeExpression (+ arm) -> :27645 isTypeAssignableToKindEx -> isTypeAssignableTo (getConstraintFromIndexedAccess :17227)
- **TSR:** crates/tsr-checker/src/binary.rs check_addition (L221-387, kind_source/has_kind flag tests)
- **Example:** `conformance/additionOperatorWithConstrainedTypeParameter` [0:11] `n += v[k]`. want `number`, got `error`
- **Example:** `conformance/additionOperatorWithConstrainedTypeParameter` [0:22] `n = n + v[k]`. want `number`, got `error`
- **Wave-1 evidence:** conformance/additionOperatorWithConstrainedTypeParameter: `n + v[k]` with T extends Record<K, number> want number got error; minimised f2 `T extends Record<string, number>` also error, `n + v["a"]` with `{a: number}` fine
- **Cases:** conformance/additionOperatorWithConstrainedTypeParameter; conformance/operatorsAndIntersectionTypes

### T9. `CONTEXTUAL-ARG-SUPER-CALL` (1; blocked 2, finished alone 2, lines 12; confidence high)

- **Root cause / port plan:** Arguments of a `super(...)` call are contextually typed via getContextualTypeForArgumentAtIndex -> getResolvedSignature(superCall) (resolveCallExpression's super arm: base constructor type with the extends type arguments). TSR leaves context-sensitive super-call args uncontextualised (arrow -> error, params any).
- **tsgo:** internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex -> checker.go:8471 resolveCallExpression (super arm)
- **TSR:** crates/tsr-checker/src/contextual.rs argument contextual type (no SuperCall callee path); calls.rs super call resolution
- **Example:** `conformance/superCallParameterContextualTyping1` [0:11] `value.toExponential()`. want `string`, got `any`
- **Example:** `conformance/superCallParameterContextualTyping1` [0:12] `value.toExponential`. want `(fractionDigits?: number) => string`, got `any`
- **Wave-1 evidence:** superCallParameterContextualTyping1: `super(value => ...)` want `(value: number) => string` got error/any; p12.ts: even non-generic `class A{constructor(m:(v:number)=>string)}` super(v=>"") -> v any, while `new G<number>(u=>"")` works
- **Cases:** conformance/superCallParameterContextualTyping1; conformance/superCallParameterContextualTyping3

### T10. `NODEBUILDER-DIVERGENT-ACCESSOR` (1; blocked 2, finished alone 2, lines 6; confidence high)

- **Root cause / port plan:** Property whose getter/setter types differ (or class accessor w/o property decl) must print as `get p(): T; set p(v: W);` pair in object/type-literal printing.
- **tsgo:** internal/checker/nodebuilderimpl.go:2524 addPropertyToElementList (accessor arm: getWriteTypeOfSymbol != getNonMissingTypeOfSymbol)
- **TSR:** crates/tsr-checker/src/printing.rs type_literal_text_at / object_literal_text_at (prints single `p: T` member)
- **Example:** `compiler/discriminateWithDivergentAccessors1` [0:15] `weirdoBox`. want `{ get done(): true; set done(v: number \| null); value: number; }`, got `{ done: true; value: number; }`
- **Example:** `compiler/discriminateWithDivergentAccessors1` [0:37] `weirdoBox2`. want `{ get done(): true; set done(v: string \| null); value: string; } \| { get done(): true; set done(v: string \| null \| undefined); value: number; }`, got `{ done: true; value: string; } \| { done: true; value: number; }`
- **Wave-1 evidence:** divergentAccessors1: want `{ get foo(): number; set foo(v: number \| string); }` / got `{ foo: number; }`
- **Cases:** compiler/discriminateWithDivergentAccessors1; compiler/divergentAccessors1

### T11. `PARSER-ASTERISK-METHOD` (1; blocked 2, finished alone 2, lines 6; confidence high)

- **Root cause / port plan:** Parser recovery: in an object literal element, a `*` token forces parseMethodDeclaration even when the name is missing and no `(`/`<` follows (`{ *{ } }`, `{ * }`). TSR only takes the method arm on `(`/`<`, so it builds a PropertyAssignment/shorthand instead and the member's type/line structure differs.
- **tsgo:** internal/parser/parser.go:5625 parseObjectLiteralElement (:5650 `asteriskToken != nil || token == ( || token == <` -> parseMethodDeclaration :1950)
- **TSR:** crates/tsr-parser/src/expression.rs parse_object_literal_element_worker (L1540 method arm ignores `asterisk`)
- **Example:** `conformance/FunctionPropertyAssignments3_es6` [0:0] `v`. want `{ ""(): Generator<never, void, unknown>; }`, got `{}`
- **Example:** `conformance/FunctionPropertyAssignments3_es6` [0:1] `{ *{ } }`. want `{ ""(): Generator<never, void, unknown>; }`, got `{}`
- **Wave-1 evidence:** conformance/FunctionPropertyAssignments3_es6: `var v = { *{ } }` want `{ ""(): Generator<never, void, unknown>; }` got `{}`; TSR emits a `{ } : {}` object-literal line (body parsed as initializer)
- **Cases:** conformance/FunctionPropertyAssignments3_es6 [contextual]; conformance/FunctionPropertyAssignments4_es6

### T12. `AWAITED-THIS-TYPE` (1; blocked 2, finished alone 2, lines 4; confidence medium)

- **Root cause / port plan:** Async function return wrapping when the body returns the polymorphic `this` type: getReturnTypeFromBody -> createPromiseReturnType(getAwaitedTypeNoAlias(this)); isAwaitedTypeNeeded reads getBaseConstraintOfType(this) = the class instance type (no `then`) -> not needed -> `Promise<this>`. TSR gaps the async arrow when the returned type is `this` (generic `T` works), i.e. its awaited-type/base-constraint road declines on the this-type parameter.
- **tsgo:** internal/checker/checker.go:20372 createPromiseReturnType / :31266 getAwaitedTypeNoAlias / :31392 isAwaitedTypeNeeded (getBaseConstraintOfType :27436 of thisType)
- **TSR:** crates/tsr-checker/src/expressions.rs is_awaited_type_needed (L3192) / awaited_type (L3130) -> base_constraint_of_type for this-type
- **Example:** `conformance/asyncArrowFunctionCapturesThis_es2017` [0:2] `fn`. want `() => Promise<this>`, got `error`
- **Example:** `conformance/asyncArrowFunctionCapturesThis_es2017` [0:3] `async () => await this`. want `() => Promise<this>`, got `error`
- **Wave-1 evidence:** conformance/asyncArrowFunctionCapturesThis_es6: `var fn = async () => await this` want `() => Promise<this>` got error; minimised `async (x: this) => x` error while `async <T>(x: T) => x` -> Promise<T>
- **Cases:** conformance/asyncArrowFunctionCapturesThis_es2017; conformance/asyncArrowFunctionCapturesThis_es6

### T13. `LOGICAL-ASSIGNMENT-OPERATOR` (1; blocked 2, finished alone 2, lines 3; confidence high)

- **Root cause / port plan:** &&=, ||=, ??= expression types (logical arms of checkBinaryLikeExpression shared with &&/||/??).
- **tsgo:** internal/checker/checker.go:12336 checkBinaryLikeExpression (arms at 12496, 12518)
- **TSR:** crates/tsr-checker/src/binary.rs:38 check_binary_expression (`_ => error` at 162; compound logical forms unhandled)
- **Example:** `conformance/logicalAssignment9` [0:2] `x.a ??= true`. want `boolean`, got `error`
- **Example:** `conformance/logicalAssignment9` [0:7] `x.a &&= false`. want `false \| undefined`, got `error`
- **Wave-1 evidence:** logicalAssignment9: x.a ??= true want boolean got error; probe z ??= true, z &&= false all error while x.a ?? true works
- **Cases:** conformance/logicalAssignment9; conformance/nullishCoalescingAssignmentVsPrivateFieldsJsEmit1

### T14. `DEFAULT-EXPORT-ALIAS-CLASS-MERGE` (1; blocked 2, finished alone 2, lines 2; confidence high)

- **Root cause / port plan:** `export default foo` (Alias) and `export default class Foo {}` merge into ONE `default` symbol (ClassExcludes does not exclude Alias); class type is created on it and printed with the first declaration's name (`foo`). TSR answers error/any.
- **tsgo:** internal/binder/binder.go:152 declareSymbolEx (no conflict -> merge) + internal/checker/nodebuilderimpl.go:973 getNameOfSymbolAsWritten (first named declaration)
- **TSR:** crates/tsr-binder/src/binder.rs default-export declare / crates/tsr-checker/src/declared.rs declared type of ALIAS|CLASS symbol
- **Example:** `compiler/exportDefaultClassAndValue` [0:3] `Foo`. want `foo`, got `any`
- **Example:** `compiler/exportDefaultTypeClassAndValue` [0:3] `Foo`. want `foo`, got `any`
- **Wave-1 evidence:** exportDefaultClassAndValue: class name `Foo` want `foo` / got any; oracle w.ts: tsgo `Foo : foo`, `new Foo() : foo`; TSR error/any
- **Cases:** compiler/exportDefaultClassAndValue; compiler/exportDefaultTypeClassAndValue

### T15. `RELATER-APPARENT-INDEX-SIGNATURE` (1; blocked 2, finished alone 2, lines 2; confidence medium)

- **Root cause / port plan:** Subtype relation from a primitive to an object type with a number index signature goes through the primitive's apparent type (String has [index: number]: string) and indexSignaturesRelatedTo; used by isConstructedBy in constructor narrowing.
- **tsgo:** internal/checker/relater.go:4578 indexSignaturesRelatedTo; flow.go:794 isConstructedBy
- **TSR:** crates/tsr-checker/src/relater.rs (index-signature subtype for primitive sources) via flow.rs is_constructed_by
- **Example:** `compiler/typeGuardConstructorNarrowPrimitivesInUnion` [0:14] `var1`. want `"hello" \| "world"`, got `never`
- **Example:** `compiler/typeGuardConstructorPrimitiveTypes` [0:6] `var1`. want `string`, got `never`
- **Wave-1 evidence:** typeGuardConstructorPrimitiveTypes 0:6: want string / got never; probed interface S4 { readonly [i: number]: string } narrows to never, S3 {length} works
- **Cases:** compiler/typeGuardConstructorNarrowPrimitivesInUnion; compiler/typeGuardConstructorPrimitiveTypes

### T16. `FLOW-INITIAL-BINDING-ELEMENT` (1; blocked 2, finished alone 2, lines 2; confidence high)

- **Root cause / port plan:** Flow assignment node for a destructuring binding element (var/let/for-of `{y: b = true}`): the initial type is getTypeWithDefault(parent element type, default) and getAssignmentReducedType narrows the declared (widened) type; TSR get_initial_or_assigned_type only handles VariableDeclaration-with-initializer and plain `x = e`, so the reference keeps the declared type.
- **tsgo:** internal/checker/flow.go:2273 getInitialTypeOfBindingElement (via flow.go:276 getInitialOrAssignedType, flow.go:2389 getTypeWithDefault)
- **TSR:** crates/tsr-checker/src/flow.rs get_initial_or_assigned_type (~L2440)
- **Example:** `conformance/for-of43` [0:15] `b`. want `number \| true`, got `number \| boolean`
- **Example:** `conformance/stringLiteralTypesAndTuples01` [0:19] `dinosaur`. want `"t-rex"`, got `RexOrRaptor`
- **Wave-1 evidence:** conformance/for-of43: `b` ref want number \| true / got number \| boolean; minimised: `var {y: c = true} = o; c;` also gives number\|boolean
- **Cases:** conformance/for-of43; conformance/stringLiteralTypesAndTuples01

### T17. `CONTEXTUAL-MAPPED-TUPLE-CONSTRAINT` (1; blocked 2, finished alone 1, lines 33; confidence medium)

- **Root cause / port plan:** An array literal contextually typed by a homomorphic mapped type over a type variable with a tuple-like constraint (`T extends {0: unknown}`, `TTypes extends readonly [T, ...T[]]`) is checked in tuple context and each element gets the substituted template (getResolvedApparentTypeOfMappedType / substituteIndexedMappedType), giving literal-preserving contextual types and tuple reverse-inference. TSR misses the per-element contextual type → literals widen, callbacks ungrounded, inference falls back to the constraint.
- **tsgo:** internal/checker/checker.go:8029 checkArrayLiteral inTupleContext; :21772 getResolvedApparentTypeOfMappedType; :29291 substituteIndexedMappedType; :30551 getTypeOfPropertyOfContextualType; inference.go:1014 createReverseMappedType
- **TSR:** crates/tsr-checker/src/array_literals.rs check_array_literal (L647); contextual.rs contextual element/property types
- **Example:** `compiler/reverseMappedIntersectionInference2` [0:12] `res`. want `[[string, boolean], [number, number]]`, got `[{ 0: unknown; }, { 0: unknown; }]`
- **Example:** `compiler/reverseMappedIntersectionInference2` [0:13] `withTupleLike([  {    data: "foo",    onSuccess: (dataArg) => {      dataArg;    },    error: 404,  `. want `[[string, boolean], [number, number]]`, got `[{ 0: unknown; }, { 0: unknown; }]`
- **Wave-1 evidence:** probe t2.ts: w4<T extends {0: unknown},...>([{data:'x',error:1},...]) tsgo `[[string, boolean], [number, number]]` / TSR `[{ 0: unknown; }, ...]` (no-constraint variant right); probe t3.ts b1([{type:'a'}]) tsgo `["a","b"]` / TSR `[string, string]`
- **Cases:** compiler/reverseMappedIntersectionInference2; compiler/reverseMappedTypeContextualTypesPerElementOfTupleConstraint

### T18. `CONTEXTUAL-ARG-GENERIC-MAPPED` (1; blocked 2, finished alone 1, lines 19; confidence medium)

- **Root cause / port plan:** Inside a generic call, an object-literal argument whose parameter type is a generic mapped type over an UNFIXED type parameter (`{[P in keyof U]: (props: X) => U[P]}`) must give each property the contextual type substituteIndexedMappedType(mapped, "name") (uninstantiated U kept), so context-sensitive member functions get typed params and U is reverse-mapped. TSR types it outside calls but in the call path the member arrow gets no contextual signature (props:any, arrow error).
- **tsgo:** internal/checker/checker.go:30551 getTypeOfPropertyOfContextualType (+ :29291 substituteIndexedMappedType, instantiateContextualType during inferTypeArguments)
- **TSR:** crates/tsr-checker/src/contextual.rs contextual_type_for_argument (inferential/active_inference_contexts road) -> mapped.rs generic_mapped_contextual_property_type
- **Example:** `compiler/genericFunctionInference2` [0:33] `enhancer4`. want `{ onChange: (e: any) => void; onSubmit: (e: any) => void; }`, got `error`
- **Example:** `compiler/genericFunctionInference2` [0:34] `withH((props: Props) => ({    onChange: (props) => (e: any) => {},    onSubmit: (props) => (e: any) `. want `{ onChange: (e: any) => void; onSubmit: (e: any) => void; }`, got `error`
- **Wave-1 evidence:** genericFunctionInference2: withH((props: Props) => ({onChange: (props) => ...})) props want Props got any; probe w1<U>(x:{[P in keyof U]:(props:Props)=>U[P]}) w1({onChange:(props)=>1}) errors, same literal assigned to annotated local is right
- **Cases:** compiler/genericFunctionInference2; compiler/mappedTypeContextualTypesApplied (+KEYOF-ANY)

### T19. `LITERAL-WIDENING` (1; blocked 2, finished alone 1, lines 6; confidence low)

- **Root cause / port plan:** Object-literal property literal types are widened (getWidenedLiteralType via checkPropertyAssignment/checkExpressionForMutableLocation) when the contextual type does not contain the literal; `{ x: isRtl ? -1 : 1 }` assigned to a declared `{x: number; y: number}`-like target prints x: number; TSR keeps `-1 | 1`
- **tsgo:** internal/checker/checker.go:13878 checkExpressionForMutableLocation -> :25487 getWidenedLiteralType / isLiteralOfContextualType :25522
- **TSR:** crates/tsr-checker/src/widening.rs / expressions.rs mutable-location widening
- **Example:** `compiler/controlFlowCaching` [0:358] `axisVector = { x: isRtl ? -1`. want `1, y: 0 } : { x: number; y: number; }`, got `1, y: 0 } : { x: -1 \| 1; y: number; }`
- **Example:** `compiler/controlFlowCaching` [0:360] `{ x: isRtl ? -1`. want `1, y: 0 } : { x: number; y: number; }`, got `1, y: 0 } : { x: -1 \| 1; y: number; }`
- **Wave-1 evidence:** compiler/controlFlowCaching: axisVector = { x: isRtl ? -1 : 1, y: 0 } want { x: number; ... } got { x: -1 \| 1; ... } (unminimised)
- **Cases:** compiler/controlFlowCaching; compiler/correlatedUnions (+FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, UNATTRIBUTED)

### T20. `DECL-NAME-MERGED-SYMBOL` (1; blocked 2, finished alone 1, lines 6; confidence medium)

- **Root cause / port plan:** The type of a declaration name is the type of the MERGED symbol (getMergedSymbol), so a local `interface Document` method shows lib + local signatures.
- **tsgo:** internal/checker/checker.go:14390 getSymbolOfDeclaration -> 14355 getMergedSymbol (getTypeOfNode 31927)
- **TSR:** crates/tsr-conformance/src/types_producer.rs declaration-name road (binder symbol, not merged); symbol_access.rs:557 merged_symbol exists
- **Example:** `compiler/overloadBindingAcrossDeclarationBoundaries2` [1:0] `a`. want `{ (o: Opt1): Opt1; (o: Opt2): Opt2; (o: Opt3): Opt3; (o: Opt4): Opt4; }`, got `{ (o: Opt3): Opt3; (o: Opt4): Opt4; }`
- **Example:** `compiler/overloadBindingAcrossDeclarationBoundaries2` [1:2] `a`. want `{ (o: Opt1): Opt1; (o: Opt2): Opt2; (o: Opt3): Opt3; (o: Opt4): Opt4; }`, got `{ (o: Opt3): Opt3; (o: Opt4): Opt4; }`
- **Wave-1 evidence:** parserOverloadOnConstants1: createElement decl name want 3 lib + 4 local sigs got 4 local; d.createElement (reference) prints merged sigs correctly
- **Cases:** compiler/overloadBindingAcrossDeclarationBoundaries2 (+REORDER-CANDIDATES); conformance/parserOverloadOnConstants1 [type-operators]

### T21. `KEYOF-ANY` (1; blocked 2, finished alone 1, lines 5; confidence high)

- **Root cause / port plan:** getIndexType(any) = keyofConstraintType (string | number | symbol); TSR's `keyof any` is error.
- **tsgo:** internal/checker/checker.go:26680 getIndexType
- **TSR:** crates/tsr-checker/src/declared.rs TypeOperator keyof arm
- **Example:** `compiler/mappedTypeContextualTypesApplied` [0:36] `{foo: s => 42}`. want `{ foo: (s: string) => number; }`, got `error`
- **Example:** `compiler/mappedTypeContextualTypesApplied` [0:37] `foo`. want `(s: string) => number`, got `error`
- **Wave-1 evidence:** probe mc2: `declare const k: keyof any` error; mapped3 contextual typing lost
- **Cases:** compiler/mappedTypeContextualTypesApplied (+CONTEXTUAL-ARG-GENERIC-MAPPED); conformance/intersectionWithUnionConstraint

### T22. `IDENT-UNDEFINED-SHADOWED` (1; blocked 2, finished alone 1, lines 3; confidence medium)

- **Root cause / port plan:** The identifier `undefined` resolves by ordinary name resolution: a local class/namespace named undefined shadows the global, and a qualifier `undefined` inside a type reference qualified name is not an expression (writer getTypeOfNode -> errorType -> any). TSR special-cases the text `undefined`.
- **tsgo:** internal/checker/checker.go:11042 checkIdentifier (undefinedSymbol identity check), :31927 getTypeOfNode
- **TSR:** crates/tsr-checker/src/expressions.rs identifier resolution / crates/tsr-conformance/src/types_producer.rs type_at_location
- **Example:** `compiler/typeNamedUndefined2` [0:13] `undefined`. want `any`, got `typeof undefined`
- **Example:** `compiler/undefinedTypeAssignment4` [0:7] `y`. want `typeof undefined`, got `undefined`
- **Wave-1 evidence:** undefinedTypeAssignment4 0:7 `var y: typeof undefined` with class undefined: want typeof undefined / got undefined; typeNamedUndefined2 0:13 qualifier `undefined` want any / got typeof undefined (`ns.a.b` qualifier gives any)
- **Cases:** compiler/typeNamedUndefined2 (+QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, RETURN-WIDEN-UNIQUE-SYMBOL); compiler/undefinedTypeAssignment4

### T23. `NODEBUILDER-SHADOWED-TYPEPARAM-CONST` (1; blocked 1, finished alone 1, lines 48; confidence medium)

- **Root cause / port plan:** Overloaded function whose overloads redeclare the same type-parameter name with a `const` modifier: the function type is the ordinary overload set; the node builder renames shadowed params (T_1, GenerateNamesForShadowedTypeParams) and prints `const`. TSR answers error for the symbol type when renaming meets a const type parameter (works without const, or with const but distinct names).
- **tsgo:** internal/checker/nodebuilderimpl.go:1404 typeParameterToName (shadowed-name generation) via signatureToSignatureDeclarationHelper; type from checker.go getTypeOfFuncClassEnumModule
- **TSR:** crates/tsr-checker/src/checker.rs type_parameter_name_at / signatures.rs type_parameter_of (const admitted) — combination declines the overload-set type
- **Example:** `conformance/typeParameterConstModifiersReturnsAndYields` [0:216] `overloaded1`. want `{ <const T>(cb: () => T): T; <const T_1, const U>(cb: () => T_1, cb2: () => U): [T_1, U]; }`, got `error`
- **Example:** `conformance/typeParameterConstModifiersReturnsAndYields` [0:218] `overloaded1`. want `{ <const T_1>(cb: () => T_1): T_1; <const T, const U>(cb: () => T, cb2: () => U): [T, U]; }`, got `error`
- **Wave-1 evidence:** typeParameterConstModifiersReturnsAndYields: `declare function overloaded1<const T>(...); declare function overloaded1<const T, const U>(...)` want `{ <const T>(...): T; <const T_1, const U>(...) }` got error; p26.ts o1 error vs o2 (no const) OK, p27.ts const without name reuse OK
- **Cases:** conformance/typeParameterConstModifiersReturnsAndYields

### T24. `APPARENT-UNKNOWN-NONSTRICT` (1; blocked 1, finished alone 1, lines 30; confidence high)

- **Root cause / port plan:** getApparentType of an unconstrained type parameter: constraint unknown -> emptyObjectType {} when strictNullChecks is off, so Object members (toString) resolve.
- **tsgo:** internal/checker/checker.go:21729 getApparentType (21758 unknown && !strictNullChecks)
- **TSR:** crates/tsr-checker/src/members.rs:1113 apparent_type
- **Example:** `conformance/propertyAccessOnTypeParameterWithoutConstraints` [0:1] `f`. want `() => string`, got `() => any`
- **Example:** `conformance/propertyAccessOnTypeParameterWithoutConstraints` [0:3] `a`. want `string`, got `error`
- **Wave-1 evidence:** propertyAccessOnTypeParameterWithoutConstraints (strict:false): x: T, x.toString want () => string got error; probe with T extends {} works
- **Cases:** conformance/propertyAccessOnTypeParameterWithoutConstraints

### T25. `SELF-REFERENTIAL-TYPE-LITERAL` (1; blocked 1, finished alone 1, lines 19; confidence medium)

- **Root cause / port plan:** A type literal whose member references the declaring variable (`var a: { foo: typeof a }`): tsgo creates ONE anonymous type with lazily resolved members (resolveAnonymousTypeMembers), so `typeof a` inside is the same type, and the nodebuilder prints the member by reusing the written `typeof a` node (serializeTypeForDeclaration -> tryReuseExistingNonParameterTypeNode). TSR resolves the inner `typeof a` eagerly with a cycle cut, producing a different, truncated structural type `{ foo: { foo: {}; }; }`.
- **tsgo:** internal/checker/checker.go:22933 getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode + :20650 resolveAnonymousTypeMembers; nodebuilderimpl.go:2181 serializeTypeForDeclaration / :542 tryReuseExistingNonParameterTypeNode
- **TSR:** crates/tsr-checker/src/declared.rs get_type_from_type_literal (eager member typing, text baked at creation) + TypeQuery self-reference cycle cut
- **Example:** `conformance/assignmentCompatWithObjectMembers` [0:126] `s2 = b`. want `{ foo: typeof b; }`, got `{ foo: { foo: {}; }; }`
- **Example:** `conformance/assignmentCompatWithObjectMembers` [0:128] `b`. want `{ foo: typeof b; }`, got `{ foo: { foo: {}; }; }`
- **Wave-1 evidence:** conformance/assignmentCompatWithObjectMembers: `var a: { foo: typeof a; }` want `{ foo: typeof a; }` got `{ foo: { foo: {}; }; }`; minimised rc.ts identical
- **Cases:** conformance/assignmentCompatWithObjectMembers

### T26. `CONTEXTUAL-PARAM` (1; blocked 1, finished alone 1, lines 16; confidence medium)

- **Root cause / port plan:** Unannotated callback parameter in object literal not contextually typed; missing source: discriminated contextual type (discriminateContextualTypeByObjectMembers on `disc: undefined` / absent disc).
- **tsgo:** internal/checker/checker.go discriminateContextualTypeByObjectMembers / getApparentTypeOfContextualType
- **TSR:** crates/tsr-checker/src/contextual.rs
- **Example:** `compiler/discriminantPropertyInference` [0:38] `{    disc: undefined,    cb: n => n.toFixed()}`. want `{ disc: undefined; cb: (n: number) => string; }`, got `{ disc: undefined; cb: (n: any) => any; }`
- **Example:** `compiler/discriminantPropertyInference` [0:41] `cb`. want `(n: number) => string`, got `(n: any) => any`
- **Wave-1 evidence:** discriminantPropertyInference: n want number / got any
- **Cases:** compiler/discriminantPropertyInference

### T27. `WRITE-TYPE-INSTANTIATION` (1; blocked 1, finished alone 1, lines 15; confidence high)

- **Root cause / port plan:** getWriteTypeOfSymbol on an instantiated property symbol must instantiate the target's setter type with the symbol mapper (getWriteTypeOfInstantiatedSymbol).
- **tsgo:** internal/checker/checker.go:16536 getWriteTypeOfInstantiatedSymbol (from getWriteTypeOfSymbol checker.go:16434)
- **TSR:** crates/tsr-checker/src/symbols.rs write_type_of_accessors (returns uninstantiated setter type)
- **Example:** `compiler/divergentAccessorsTypes7` [0:10] `a.value = (item) => item.property`. want `(item: { property: string; }) => string`, got `error`
- **Example:** `compiler/divergentAccessorsTypes7` [0:11] `a.value`. want `string \| ((item: { property: string; }) => string)`, got `string \| ((item: S) => string)`
- **Wave-1 evidence:** divergentAccessorsTypes7: `a.value` want `string \| ((item: { property: string; }) => string)` / got `string \| ((item: S) => string)`; probe class T<S>{set v(x:string\|S)} `a.v` (a: T<number>) -> `string \| S`
- **Cases:** compiler/divergentAccessorsTypes7

### T28. `INFERENCE-INDEXED-ACCESS` (1; blocked 1, finished alone 1, lines 14; confidence high)

- **Root cause / port plan:** inferFromTypes arm: source and target both IndexedAccess (A[B] -> T[K]) infer objectType->objectType and indexType->indexType. TSR lacks it, so find<T,K extends keyof T>(o: T[K]) called with A[B] fails (error) and the enclosing function's return type becomes any.
- **tsgo:** internal/checker/inference.go:240 inferFromTypes (IndexedAccess/IndexedAccess arm; func at :65)
- **TSR:** crates/tsr-checker/src/inference.rs infer_from_types (no indexed-access pair arm)
- **Example:** `compiler/indexedAccessCanBeHighOrder` [0:13] `find(item)`. want `[A, B]`, got `error`
- **Example:** `compiler/indexedAccessCanBeHighOrder` [0:20] `r`. want `[{ x: number; }, "x"]`, got `any`
- **Wave-1 evidence:** indexedAccessCanBeHighOrder: find(item) with item: A[B] want [A, B] got error; probe impl<A,B>(a: A[B]) { return find(a) } -> error
- **Cases:** compiler/indexedAccessCanBeHighOrder

### T29. `LAZY-FUNCEXPR-RETURN-CIRCULARITY` (1; blocked 1, finished alone 1, lines 12; confidence medium)

- **Root cause / port plan:** A function expression's type is an anonymous type whose signature return type is resolved lazily (getReturnTypeOfSignature with its own resolution stack); a variable initialized with an arrow that calls the enclosing function does not become circular. TSR resolves the arrow's return while typing the variable → circularity → any.
- **tsgo:** internal/checker/checker.go:10114 checkFunctionExpressionOrObjectLiteralMethod; :20001 getReturnTypeOfSignature; :20126 getReturnTypeFromBody
- **TSR:** crates/tsr-checker/src/signatures.rs get_type_of_function_expression (L6149) / get_return_type_from_body (L2407)
- **Example:** `compiler/returnInfiniteIntersection` [0:0] `recursive`. want `() => (<T>(subkey: T) => any & { p: any; }) & { p: any; }`, got `() => any`
- **Example:** `compiler/returnInfiniteIntersection` [0:1] `x`. want `<T>(subkey: T) => any & { p: any; }`, got `any`
- **Wave-1 evidence:** returnInfiniteIntersection `x` want `<T>(subkey: T) => any & { p: any; }` / got any; `recursive` want `() => (<T>(subkey: T) => any & { p: any; }) & { p: any; }` / got `() => any`
- **Cases:** compiler/returnInfiniteIntersection

### T30. `NARROW-EQUALITY-REPLACE-PRIMITIVES` (1; blocked 1, finished alone 1, lines 10; confidence high)

- **Root cause / port plan:** narrowTypeByEquality assume-true arm must call replacePrimitivesWithLiterals even when the comparable filter keeps every constituent (string === "foo" -> "foo").
- **tsgo:** internal/checker/flow.go:556 narrowTypeByEquality -> flow.go:1907 replacePrimitivesWithLiterals
- **TSR:** crates/tsr-checker/src/flow.rs:7356-7365 (returns t when kept.len()==total, skipping replace_primitives_with_literals)
- **Example:** `conformance/literalTypes3` [0:5] `s`. want `"foo"`, got `string`
- **Example:** `conformance/literalTypes3` [0:13] `s`. want `"bar" \| "foo"`, got `string`
- **Wave-1 evidence:** literalTypes3: if (s === "foo") s want `"foo"` got `string`; x === 1 \|\| x === 2 want `1 \| 2`
- **Cases:** conformance/literalTypes3

### T31. `NOLIB-GLOBAL-TYPE-FALLBACK` (1; blocked 1, finished alone 1, lines 9; confidence high)

- **Root cause / port plan:** missing global types fall back to emptyGenericType/emptyObjectType (array types print `{}`)
- **tsgo:** internal/checker/checker.go:1210 getGlobalType
- **TSR:** crates/tsr-checker/src/intrinsics/global type lookup
- **Example:** `compiler/noCrashOnNoLib` [0:1] `e`. want `{}`, got `any`
- **Example:** `compiler/noCrashOnNoLib` [0:10] `e`. want `{}`, got `any`
- **Wave-1 evidence:** noCrashOnNoLib: e want {} / got any
- **Cases:** compiler/noCrashOnNoLib

### T32. `RETURN-LITERAL-SELF-CONTEXTUAL-SIG` (1; blocked 1, finished alone 1, lines 9; confidence medium)

- **Root cause / port plan:** getReturnTypeFromBody: if the function's contextual signature IS its own signature (method in an object literal passed to a bare type parameter, which is inferred from the literal itself), the unit return type is kept (`foo(): true`). TSR widens to boolean.
- **tsgo:** internal/checker/checker.go:20203-20222 getReturnTypeFromBody (case contextualSignature == getSignatureFromDeclaration(fn))
- **TSR:** crates/tsr-checker/src/signatures.rs contextual_return_widening_type (contextual.rs L1560) as used at signatures.rs L3678/L3719
- **Example:** `compiler/silentNeverPropagation` [0:11] `breaks`. want `ModuleWithState<{ a: number; } & MoreState> & ModuleWithState<{ a: number; }> & { foo(): true; }`, got `ModuleWithState<{ a: number; } & MoreState> & ModuleWithState<{ a: number; }> & { foo(): boolean; }`
- **Example:** `compiler/silentNeverPropagation` [0:12] `convert(    createModule({ a: 12 }, { foo() { return true } }))`. want `ModuleWithState<{ a: number; } & MoreState> & ModuleWithState<{ a: number; }> & { foo(): true; }`, got `ModuleWithState<{ a: number; } & MoreState> & ModuleWithState<{ a: number; }> & { foo(): boolean; }`
- **Wave-1 evidence:** silentNeverPropagation `breaks.foo()` want `true` / got boolean; probe aa.ts cm2({ foo() { return true } }) tsgo `{ foo(): true; }` / TSR boolean; plain `const a4 = {foo(){return true}}` boolean in both
- **Cases:** compiler/silentNeverPropagation

### T33. `ARRAY-LITERAL-TUPLE-LIKE-CONTEXT` (1; blocked 1, finished alone 1, lines 7; confidence high)

- **Root cause / port plan:** Array literal is in tuple context when the contextual type isTupleLikeType, which includes any type with a property "0" (RegExpMatchArray), not only tuples.
- **tsgo:** internal/checker/checker.go:23544 isTupleLikeType (used by checkArrayLiteral inTupleContext)
- **TSR:** crates/tsr-checker/src/array_literals.rs:608 array_literal_has_a_tuple_contextual_type (tuple_element_lists only)
- **Example:** `compiler/bestChoiceType` [0:2] `(''.match(/ /) \|\| [])`. want `RegExpMatchArray \| []`, got `never[] \| RegExpMatchArray`
- **Example:** `compiler/bestChoiceType` [0:24] `y`. want `RegExpMatchArray \| []`, got `never[] \| RegExpMatchArray`
- **Wave-1 evidence:** bestChoiceType `x \|\| []` want `RegExpMatchArray \| []` got `never[] \| RegExpMatchArray`; f(x: RegExpMatchArray\|number); f([]) -> tsgo []
- **Cases:** compiler/bestChoiceType

### T34. `CONTEXTUAL-NESTED-OPTIONAL-PROPERTY` (1; blocked 1, finished alone 1, lines 7; confidence high)

- **Root cause / port plan:** getContextualTypeForObjectLiteralElement -> getTypeOfPropertyOfContextualType(mapType over `T | undefined`) for an object literal nested under an OPTIONAL property whose own member is also optional (`components?: { a?: F }`): the nested literal's contextual type is `{a?: F} | undefined`, member `a` gets `F | undefined`, and function members are contextually typed by F. TSR answers error for the whole nested literal (params any) only in this optional-under-optional shape
- **tsgo:** internal/checker/checker.go:29920 getContextualTypeForObjectLiteralElement -> :30555 getTypeOfPropertyOfContextualTypeEx (mapType skipping undefined)
- **TSR:** crates/tsr-checker/src/contextual.rs contextual_type_for_object_literal_named_element / union_contextual_property_type (~L2143/L2259)
- **Example:** `compiler/contextualTypeBasedOnIntersectionWithAnyInTheMix2` [0:14] `{  components: {    a(props) {      return null;    },    div(props) {      return null;    },  },}`. want `{ components: { a(props: { href?: string; }): null; div(props: { dir?: string; }): null; }; }`, got `{ components: { a(props: any): null; div(props: any): null; }; }`
- **Example:** `compiler/contextualTypeBasedOnIntersectionWithAnyInTheMix2` [0:15] `components`. want `{ a(props: { href?: string; }): null; div(props: { dir?: string; }): null; }`, got `{ a(props: any): null; div(props: any): null; }`
- **Wave-1 evidence:** oracle: `interface P6 { components?: { a?: (props: {href?: string}) => unknown } }; y = { components: { a: (props) => null } }` tsgo props `{ href?: string; }`, TSR error/any; either level non-optional works in TSR
- **Cases:** compiler/contextualTypeBasedOnIntersectionWithAnyInTheMix2

### T35. `CONTEXTUAL-LITERAL-TYPE-PARAM-CONSTRAINT` (1; blocked 1, finished alone 1, lines 7; confidence high)

- **Root cause / port plan:** Object-literal property initializer literal is kept (not widened) when its contextual type, obtained through a type-parameter contextual type's constraint (getApparentTypeOfContextualType -> property of constraint), is a literal union (isLiteralOfContextualType). TSR widens `"1"` to string, inference candidate fails constraint and T falls back to the constraint.
- **tsgo:** internal/checker/checker.go:25515 getWidenedLiteralLikeTypeForContextualType / 25522 isLiteralOfContextualType (via checkExpressionForMutableLocation 13878, getApparentTypeOfContextualType 30686)
- **TSR:** crates/tsr-checker/src/contextual.rs contextual type of object-literal property under a type-parameter contextual type / widening.rs
- **Example:** `compiler/freshLiteralInference` [0:25] `obj3`. want `{ value: "1"; }`, got `{ value: "1" \| "2" \| "3"; }`
- **Example:** `compiler/freshLiteralInference` [0:26] `f3({ value: "1" })`. want `{ value: "1"; }`, got `{ value: "1" \| "2" \| "3"; }`
- **Wave-1 evidence:** freshLiteralInference: f3<T extends {value:"1"\|"2"\|"3"}>({value:"1"}) want `{ value: "1"; }` / got `{ value: "1" \| "2" \| "3"; }`, property value want "1" got string; oracle z.ts same
- **Cases:** compiler/freshLiteralInference

### T36. `DEFERRED-TYPE-LITERAL-MEMBERS` (1; blocked 1, finished alone 1, lines 6; confidence medium)

- **Root cause / port plan:** Type-literal member types are resolved lazily (resolveAnonymousTypeMembers), so `declare var a: {prop:number}|{prop:T27}; type T27 = typeof a` is not circular.
- **tsgo:** internal/checker/checker.go:22933 getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode, :20650 resolveAnonymousTypeMembers
- **TSR:** crates/tsr-checker/src/declared.rs get_type_from_type_literal (eager member types -> circularity)
- **Example:** `compiler/unionTypeWithRecursiveSubtypeReduction3` [0:0] `a27`. want `{ prop: number; } \| { prop: T27; }`, got `any`
- **Example:** `compiler/unionTypeWithRecursiveSubtypeReduction3` [0:2] `prop`. want `{ prop: number; } \| { prop: T27; }`, got `any`
- **Wave-1 evidence:** unionTypeWithRecursiveSubtypeReduction3 0:0 a27: want { prop: number; } \| { prop: T27; } / got any
- **Cases:** compiler/unionTypeWithRecursiveSubtypeReduction3

### T37. `FLOW-ASSIGNED-FOROF` (1; blocked 1, finished alone 1, lines 6; confidence high)

- **Root cause / port plan:** Flow assignment type for an existing variable used as a for-of/for-in initializer (`for (x of arr)`): upstream's getAssignedType ForOf arm answers the iterated element type (checkRightHandSideOfForOf), for-in answers string; TSR's get_initial_or_assigned_type returns None for these forms so the reference keeps its declared type.
- **tsgo:** internal/checker/flow.go:2288 getAssignedType (KindForOfStatement arm :2293, KindForInStatement)
- **TSR:** crates/tsr-checker/src/flow.rs get_initial_or_assigned_type (only VariableDeclaration initializer and plain `x = e`)
- **Example:** `conformance/controlFlowForOfStatement` [0:10] `x.toExponential()`. want `string`, got `error`
- **Example:** `conformance/controlFlowForOfStatement` [0:11] `x.toExponential`. want `(fractionDigits?: number) => string`, got `error`
- **Wave-1 evidence:** controlFlowForOfStatement: for (x of obj /*number[]*/) { x = x.toExponential() } x want number / got string \| number \| boolean \| RegExp; probe n.ts reproduces
- **Cases:** conformance/controlFlowForOfStatement

### T38. `ASSIGN-READONLY-ERRORTYPE` (1; blocked 1, finished alone 1, lines 6; confidence high)

- **Root cause / port plan:** Assignment target that is an import alias (checkIdentifier: Cannot_assign_to_0_because_it_is_an_import) or a readonly entity such as a namespace-import member (checkPropertyAccessExpressionOrQualifiedName isAssignmentToReadonlyEntity) returns errorType for the target expression (printed `any`). TSR returns the declared type.
- **tsgo:** internal/checker/checker.go:11042 checkIdentifier (~L11088-11094) and checker.go:11258 checkPropertyAccessExpressionOrQualifiedName (~L11376-11378)
- **TSR:** crates/tsr-checker/src/expressions.rs identifier / property-access check in assignment-target position
- **Example:** `conformance/importsImplicitlyReadonly` [0:10] `y`. want `any`, got `number`
- **Example:** `conformance/importsImplicitlyReadonly` [0:13] `a1.x`. want `any`, got `number`
- **Wave-1 evidence:** conformance/importsImplicitlyReadonly: `x = 1` x want any / got number; `a1.x = 1` a1.x want any / got number
- **Cases:** conformance/importsImplicitlyReadonly

### T39. `ACCESSORS-MERGED-PROPERTY` (1; blocked 1, finished alone 1, lines 6; confidence high)

- **Root cause / port plan:** Symbol merging a PropertyDeclaration with get/set accessors (binder allows it): getTypeOfSymbol takes the Accessor arm first; getTypeOfAccessors ignores non-accessor declarations and infers from the getter body.
- **tsgo:** internal/checker/checker.go:16506 getTypeOfSymbol (Accessor arm before Property) -> checker.go:18511 getTypeOfAccessors
- **TSR:** crates/tsr-checker/src/symbols.rs get_type_of_accessors_worker (`other => error` at ~L342)
- **Example:** `conformance/propertyAndAccessorWithSameName` [0:1] `x`. want `number`, got `any`
- **Example:** `conformance/propertyAndAccessorWithSameName` [0:2] `x`. want `number`, got `any`
- **Wave-1 evidence:** propertyAndAccessorWithSameName: `x: number; get x(){return 1}` want number/number, got any (error); minimised: without the property decl TSR answers number.
- **Cases:** conformance/propertyAndAccessorWithSameName

### T40. `CONTEXTUAL-MODULE-EXPORTS-ASSIGN` (1; blocked 1, finished alone 1, lines 5; confidence medium)

- **Root cause / port plan:** RHS of `module.exports = expr` has no contextual type; TSR derives one from the left side (circular), erroring function expressions.
- **tsgo:** internal/checker/checker.go:29820 getContextualTypeForBinaryOperand / 29850 getContextualTypeForAssignmentExpression
- **TSR:** crates/tsr-checker/src/contextual.rs binary-operand contextual type
- **Example:** `compiler/jsDeclarationEmitExportAssignedFunctionWithExtraTypedefsMembers` [0:0] `module.exports = function loader(options) {}`. want `(options: Options) => void`, got `error`
- **Example:** `compiler/jsDeclarationEmitExportAssignedFunctionWithExtraTypedefsMembers` [0:1] `module.exports`. want `(options: Options) => void`, got `error`
- **Wave-1 evidence:** probe je3: `module.exports = function(x){return 1}` TSR error, tsgo `(x: any) => number`; `=1` fine
- **Cases:** compiler/jsDeclarationEmitExportAssignedFunctionWithExtraTypedefsMembers

### T41. `INFERENCE-APPARENT-SOURCE` (1; blocked 1, finished alone 1, lines 5; confidence high)

- **Root cause / port plan:** inferFromTypes default arm: a primitive source is replaced by its apparent type (String) before inferring to object targets (Iterable<T>).
- **tsgo:** internal/checker/inference.go:267 inferFromTypes (getApparentType)
- **TSR:** crates/tsr-checker/src/inference.rs
- **Example:** `compiler/mapGroupBy` [0:17] `chars`. want `Map<string, string[]>`, got `Map<unknown, unknown[]>`
- **Example:** `compiler/mapGroupBy` [0:18] `Map.groupBy('a string', c => c)`. want `Map<string, string[]>`, got `Map<unknown, unknown[]>`
- **Wave-1 evidence:** probe it.ts: `f<T>(x: Iterable<T>); f('abc')` TSR error; mapGroupBy c: unknown
- **Cases:** compiler/mapGroupBy

### T42. `INTERSECTION-UNION-DISTRIBUTE-REDUCE` (1; blocked 1, finished alone 1, lines 5; confidence high)

- **Root cause / port plan:** getIntersectionType distributes over a union constituent and getReducedType removes members with never discriminant props
- **tsgo:** internal/checker/checker.go:26056 getIntersectionTypeEx / :21819 getReducedType
- **TSR:** crates/tsr-checker/src/intersection construction
- **Example:** `compiler/objectAssignLikeNonUnionResult` [0:25] `t1`. want `[R1: Interface & { field: number; }]`, got `t1`
- **Example:** `compiler/objectAssignLikeNonUnionResult` [0:26] `data1`. want `Interface & { field: number; }`, got `Interface & ({ field: number; } \| { field?: undefined; })`
- **Wave-1 evidence:** objectAssignLikeNonUnionResult: data1 want Interface & { field: number; } / got Interface & ({...}\|{field?: undefined}); oracle confirmed
- **Cases:** compiler/objectAssignLikeNonUnionResult

### T43. `CONTEXTUAL-DISCRIMINATE-OBJLIT` (1; blocked 1, finished alone 1, lines 5; confidence medium)

- **Root cause / port plan:** discriminateContextualTypeByObjectMembers narrows an object literal's union contextual type using possibly-discriminant initializers (incl. template expressions) via discriminateTypeByDiscriminableItems/assignability, not only literal units.
- **tsgo:** internal/checker/checker.go:30755 discriminateContextualTypeByObjectMembers, :30802 isPossiblyDiscriminantValue
- **TSR:** crates/tsr-checker/src/symbols.rs discriminate_union_root (string/numeric/boolean literal initializers only)
- **Example:** `compiler/templateExpressionAsPossiblyDiscriminantValue` [0:10] `{  href: `2${undefined}332132`,  onClick: (ev) => console.log('@@@@', ev),}`. want `{ href: string; onClick: (ev: string) => void; }`, got `error`
- **Example:** `compiler/templateExpressionAsPossiblyDiscriminantValue` [0:14] `onClick`. want `(ev: string) => void`, got `error`
- **Wave-1 evidence:** templateExpressionAsPossiblyDiscriminantValue 0:15: want (ev: string) => void / got error (href: `2${undefined}...` should select BiomePlainLinkProps)
- **Cases:** compiler/templateExpressionAsPossiblyDiscriminantValue

### T44. `TYPELIT-PRIVATE-NAME-MEMBER` (1; blocked 1, finished alone 1, lines 5; confidence low)

- **Root cause / port plan:** Type literals/interfaces containing #private members (a grammar error) still resolve: members get mangled symbols and nested ones print; TSR answers error for the whole type literal.
- **tsgo:** internal/binder (private-name symbol names) + checker resolveAnonymousTypeMembers; nodebuilder member printing
- **TSR:** crates/tsr-checker/src/declared.rs type-literal arm (PrivateIdentifier member -> error)
- **Example:** `conformance/privateNameAndPropertySignature` [0:0] `A`. want `A`, got `any`
- **Example:** `conformance/privateNameAndPropertySignature` [0:5] `x`. want `{ bar: { #baz: string; #taz(): string; }; }`, got `any`
- **Wave-1 evidence:** privateNameAndPropertySignature: type A = { #foo: string } want A got any; x want { bar: { #baz: string; #taz(): string; }; } got any
- **Cases:** conformance/privateNameAndPropertySignature

### T45. `RESOLVE-DECORATOR-SCOPE` (1; blocked 1, finished alone 1, lines 4; confidence high)

- **Root cause / port plan:** Names in decorators resolve outside the decorated declaration: class decorators don't see class type params; method/parameter decorators don't see the method's parameters.
- **tsgo:** internal/binder/nameresolver.go:245 (KindDecorator arm in Resolve)
- **TSR:** crates/tsr-binder resolve_name (no decorator arm)
- **Example:** `compiler/decoratorReferences` [0:10] `y(null as T)`. want `any`, got `error`
- **Example:** `compiler/decoratorReferences` [0:11] `y`. want `(...args: any[]) => any`, got `any`
- **Wave-1 evidence:** decoratorReferences: `@y(1 as T)` want number / got T; `@y x` y want function / got any
- **Cases:** compiler/decoratorReferences

### T46. `VAR-TYPE-FROM-VALUE-DECLARATION` (1; blocked 1, finished alone 1, lines 4; confidence high)

- **Root cause / port plan:** Type of a merged (redeclared) var symbol comes from symbol.ValueDeclaration (the FIRST declaration) for every declaration/reference; TSR types each declaration from its own initializer.
- **tsgo:** internal/checker/checker.go:16578 getTypeOfVariableOrParameterOrPropertyWorker (uses symbol.ValueDeclaration)
- **TSR:** crates/tsr-checker/src/symbols.rs type of variable symbol per declaration (declaration-local initializer)
- **Example:** `compiler/duplicateVarsAcrossFileBoundaries` [1:0] `x`. want `number`, got `boolean`
- **Example:** `compiler/duplicateVarsAcrossFileBoundaries` [2:0] `x`. want `number`, got `string`
- **Wave-1 evidence:** duplicateVarsAcrossFileBoundaries: file1 `var x = true` want number / got boolean (first decl `var x = 3` in file0)
- **Cases:** compiler/duplicateVarsAcrossFileBoundaries

### T47. `NARROW-INSTANCEOF-CONSTRUCT-SIGNATURES` (1; blocked 1, finished alone 1, lines 4; confidence high)

- **Root cause / port plan:** instanceof narrowing with a right operand that is not a class constructor: getInstanceType uses the `prototype` property type, else the union of construct-signature return types (type literal `{ new(): I }`, intersection `{new(): I} & {foo: true}`). TSR routes Anonymous type-literal callees to the class-identity path and finds no signature candidates on intersections, so the reference is not narrowed.
- **tsgo:** internal/checker/flow.go:811 narrowTypeByInstanceof -> :966 getInstanceType
- **TSR:** crates/tsr-checker/src/flow.rs instanceof arm (~L5127-5274: TypeData::Anonymous class-identity branch / signature_candidates_of_named_type)
- **Example:** `compiler/inKeywordAndIntersection` [0:27] `instance.one()`. want `void`, got `error`
- **Example:** `compiler/inKeywordAndIntersection` [0:28] `instance.one`. want `() => void`, got `error`
- **Wave-1 evidence:** inKeywordAndIntersection: instance instanceof ClassOne (ClassOne: {new(): InstanceOne} & {foo: true}) want InstanceOne got InstanceOne \| InstanceTwo; probe with plain `{ new(): I1 }` also unnarrowed
- **Cases:** compiler/inKeywordAndIntersection

### T48. `GENERIC-INTERSECTION-ITERATION-ARRAYLIKE` (1; blocked 1, finished alone 1, lines 4; confidence n/a)

- **Root cause / port plan:** 
- **tsgo:** 
- **TSR:** 
- **Example:** `compiler/narrowingTypeofUndefined2` [0:13] `p`. want `unknown`, got `any`
- **Example:** `compiler/narrowingTypeofUndefined2` [0:15] `m`. want `(T & {})[number][]`, got `error`
- **Cases:** compiler/narrowingTypeofUndefined2

### T49. `INFERENCE-KEYOF-LITERAL` (1; blocked 1, finished alone 1, lines 4; confidence high)

- **Root cause / port plan:** inferFromTypes with an index-type target (`keyof T`) and a string-literal source: infers createEmptyObjectTypeFromStringLiteral(source) (`{ a: any }`) to T with InferencePriorityLiteralKeyof, contravariantly, so two arguments give `{ a: any; } & { b: any; }` (beats the T = X default). TSR has no such arm -> call error.
- **tsgo:** internal/checker/inference.go:238 inferFromTypes keyof arm (+ :1229 createEmptyObjectTypeFromStringLiteral)
- **TSR:** crates/tsr-checker/src/inference.rs infer_from_types (no keyof-target arm)
- **Example:** `conformance/keyofInferenceIntersectsResults` [0:13] `b`. want `{ a: any; } & { b: any; }`, got `error`
- **Example:** `conformance/keyofInferenceIntersectsResults` [0:14] `foo('a', 'b')`. want `{ a: any; } & { b: any; }`, got `error`
- **Wave-1 evidence:** conformance/keyofInferenceIntersectsResults: bar('a', 'b') want { a: any; } & { b: any; } / got error
- **Cases:** conformance/keyofInferenceIntersectsResults

### T50. `TYPEFACTS-NONSTRICT-PRIMITIVES` (1; blocked 1, finished alone 1, lines 4; confidence high)

- **Root cause / port plan:** getTypeFactsWorker picks the non-strict fact tables (Base*Facts incl. Falsy/EQUndefined/EQNull) for primitives & literals when strictNullChecks is off, so falsy-narrowing keeps truthy literals.
- **tsgo:** internal/checker/checker.go:30982 getTypeFactsWorker (strictNullChecks ? *StrictFacts : *Facts)
- **TSR:** crates/tsr-checker/src/flow.rs get_type_facts (literal/primitive arms use strict facts unconditionally, ~L8105)
- **Example:** `conformance/stringLiteralTypesInUnionTypes04` [0:30] `f`. want `T`, got `""`
- **Example:** `conformance/stringLiteralTypesInUnionTypes04` [0:31] `x`. want `T`, got `""`
- **Wave-1 evidence:** stringLiteralTypesInUnionTypes04 (strict:false): `if (!x) { let f = x }` want T got ""
- **Cases:** conformance/stringLiteralTypesInUnionTypes04

### T51. `ARGUMENTS-NAME-RESOLVE` (1; blocked 1, finished alone 1, lines 3; confidence medium)

- **Root cause / port plan:** `arguments` resolves to argumentsSymbol only inside non-arrow function-like declarations (nameresolver); elsewhere (interface members, function types, top level) it is unresolved -> errorType (prints any) and signatures still print with the reused `typeof arguments` node.
- **tsgo:** internal/binder/nameresolver.go:228 (arguments arm)
- **TSR:** crates/tsr-checker/src/expressions.rs:932-981 (unresolved `arguments` answers gap error, poisoning the signature)
- **Example:** `compiler/arguments` [0:13] `method`. want `(args: typeof arguments) => void`, got `any`
- **Example:** `compiler/arguments` [0:16] `fn`. want `(args: typeof arguments) => void`, got `any`
- **Wave-1 evidence:** arguments: want `method : (args: typeof arguments) => void` got error; `typeof undefinedThing` in same position is right
- **Cases:** compiler/arguments

### T52. `INSTANTIATE-SINGLE-GENERIC-CALL-SIGNATURE` (1; blocked 1, finished alone 1, lines 3; confidence high)

- **Root cause / port plan:** A generic-function-typed expression (identity) in a position whose contextual type is a single non-generic signature is instantiated in the context of that signature (instantiateSignatureInContextOf), including when the context is a nested property of a function's contextual return type during inference; this lets U infer string. TSR only handles the direct-argument propagation branch, so the nested object-literal member stays <V>(y: V) => V and U = unknown.
- **tsgo:** internal/checker/checker.go:7599 instantiateTypeWithSingleGenericCallSignature
- **TSR:** crates/tsr-checker/src/inference.rs instantiateTypeWithSingleGenericCallSignature port (~L2241/2296)
- **Example:** `compiler/inferentialTypingWithFunctionTypeNested` [0:11] `() => { return { x: identity }; }`. want `() => { x: (y: string) => string; }`, got `() => { x: <V>(y: V) => V; }`
- **Example:** `compiler/inferentialTypingWithFunctionTypeNested` [0:7] `s`. want `string`, got `unknown`
- **Wave-1 evidence:** inferentialTypingWithFunctionTypeNested: map("", () => { return { x: identity }; }) want string got unknown; oracle probe: map2("", identity) right, nested object-literal form wrong
- **Cases:** compiler/inferentialTypingWithFunctionTypeNested

### T53. `APPARENT-INDEXED-ACCESS-MAPPED-INTERSECTION` (1; blocked 1, finished alone 1, lines 3; confidence low)

- **Root cause / port plan:** Property access/destructuring on a generic indexed access whose object is an intersection of generic mapped types `(E<T> & S<T>)[keyof T]` resolves members through getApparentType/base constraint (mapped substitution per constituent -> T[K] & {...}); TSR finds no properties (single mapped `E<T>[keyof T]` works).
- **tsgo:** internal/checker/checker.go:27915 getSimplifiedIndexedAccessType / computeBaseConstraint IndexedAccess arm (getApparentType)
- **TSR:** crates/tsr-checker/src/constraints.rs compute_base_constraint / mapped.rs indexed mapped substitution
- **Example:** `compiler/genericIndexedAccessMethodIntersectionCanBeAccessed` [0:24] `__$daemonMode`. want `string \| undefined`, got `error`
- **Example:** `compiler/genericIndexedAccessMethodIntersectionCanBeAccessed` [0:25] `__$action`. want `string \| undefined`, got `error`
- **Wave-1 evidence:** genericIndexedAccessMethodIntersectionCanBeAccessed: const {__$daemonMode, __$action, id} = method want string \| undefined got error; probe m.a on (E<T>&S<T>)[keyof T] error, on E<T>[keyof T] right
- **Cases:** compiler/genericIndexedAccessMethodIntersectionCanBeAccessed

### T54. `CONTEXTUAL-UNION-PROPERTY` (1; blocked 1, finished alone 1, lines 3; confidence medium)

- **Root cause / port plan:** getTypeOfPropertyOfContextualType over a union contextual type maps constituents and skips those lacking the property, so `convert` gets (t: string) => string from the one constituent that declares it.
- **tsgo:** internal/checker/checker.go:30551 getTypeOfPropertyOfContextualType
- **TSR:** crates/tsr-checker/src/contextual.rs union_contextual_property_type / contextual_property_type
- **Example:** `compiler/tsxInferenceShouldNotYieldAnyOnUnions` [0:16] `{ data: "1", convert: n => "" + n }`. want `{ data: string; convert: (n: string) => string; }`, got `any`
- **Example:** `compiler/tsxInferenceShouldNotYieldAnyOnUnions` [0:19] `convert`. want `(n: string) => string`, got `any`
- **Wave-1 evidence:** tsxInferenceShouldNotYieldAnyOnUnions 0:20: want (n: string) => string / got any; probed non-generic N(props: Props<string>) also fails
- **Cases:** compiler/tsxInferenceShouldNotYieldAnyOnUnions

### T55. `COMPARABLE-TEMPLATE-LITERAL` (1; blocked 1, finished alone 1, lines 3; confidence medium)

- **Root cause / port plan:** Discriminant narrowing where a constituent's discriminant property is a template-literal type (`${string} is wrong!`): upstream's narrowTypeByDiscriminant filter asks isTypeComparableTo(narrowedPropType, discriminantType), which the relater decides for template literals. TSR's relater answers Unknown for the template-literal pair, and narrow_type_by_discriminant aborts (`None => return t`), leaving the union un-narrowed.
- **tsgo:** internal/checker/flow.go:725 narrowTypeByDiscriminant (isTypeComparableTo filter) / relater.go templateLiteral relations
- **TSR:** crates/tsr-checker/src/flow.rs narrow_type_by_discriminant -> discriminant_keeps (relate_ternary Unknown -> None -> return t)
- **Example:** `conformance/discriminatedUnionTypes3` [0:16] `example.property`. want `true`, got `error`
- **Example:** `conformance/discriminatedUnionTypes3` [0:17] `example`. want `Correct`, got `SomeReturnType`
- **Wave-1 evidence:** discriminatedUnionTypes3: if (example.err === undefined) example want Correct / got SomeReturnType; probe: same shape with `err: string` or `err: "a"` narrows, template literal does not
- **Cases:** conformance/discriminatedUnionTypes3

### T56. `GENERIC-ARG-FINAL-RECHECK` (1; blocked 1, finished alone 1, lines 3; confidence medium)

- **Root cause / port plan:** Expression types inside a generic call's (context-sensitive/object-literal) argument must reflect the FINAL contextual type (instantiated signature); TSR records the types computed during the inferential pass (uninstantiated T -> object constraint, no literal context).
- **tsgo:** internal/checker/checker.go:8843 resolveCall final pass / 7484 checkExpressionWithContextualType; getContextualThisParameterType 12021
- **TSR:** crates/tsr-checker/src/calls.rs / inference.rs inferential argument checking (node-type cache keeps first-pass answers)
- **Example:** `conformance/neverReturningFunctions1` [0:281] `[true]`. want `true[]`, got `boolean[]`
- **Example:** `conformance/neverReturningFunctions1` [0:317] `this`. want `{ schema: { myProperty: { default: never[]; parse(): boolean[]; }; string: { type: string; }; num: number; }; init(): void; update(): void; tick(): void; remove`, got `object & Component<any>`
- **Wave-1 evidence:** neverReturningFunctions1: this in multiply() want `{schema...} & Component<any>` got `object & Component<any>`; reg2({parse(){return [true]}}) [true] want true[] (oracle) got boolean[]
- **Cases:** conformance/neverReturningFunctions1

### T57. `NODEBUILDER-ANON-CLASS-NAME` (1; blocked 1, finished alone 1, lines 3; confidence high)

- **Root cause / port plan:** Type-reference printing of a nameless class-expression symbol uses getNameOfSymbolAsWritten's nameless-declaration arm -> `(Anonymous class)`; TSR applies it only to the `typeof` leg, so the instance reference prints the internal `__class`.
- **tsgo:** internal/checker/nodebuilderimpl.go:1004-1014 getNameOfSymbolAsWritten
- **TSR:** crates/tsr-checker/src/symbols.rs anonymous_class_written_name (not used by the generic type-reference printer in printing.rs)
- **Example:** `conformance/staticIndexSignature6` [0:26] `c`. want `(Anonymous class)<number>`, got `__class<number>`
- **Example:** `conformance/staticIndexSignature6` [0:27] `new C<number>()`. want `(Anonymous class)<number>`, got `__class<number>`
- **Wave-1 evidence:** staticIndexSignature6: `new C<number>()` want `(Anonymous class)<number>` got `__class<number>`
- **Cases:** conformance/staticIndexSignature6

### T58. `INFERENCE-CLASS-STATIC-CONSTRUCT-SIGS` (1; blocked 1, finished alone 1, lines 3; confidence medium)

- **Root cause / port plan:** inferFromObjectTypes -> inferFromSignatures(kind Construct) when the TARGET is a class constructor type (static side of a generic-scoped class expression): its default construct signature (getDefaultConstructSignatures) returns the instance type whose members mention T. TSR does not produce construct signatures for that target in inference.
- **tsgo:** internal/checker/inference.go:699 inferFromObjectTypes -> inference.go:838 inferFromSignatures (SignatureKindConstruct); default construct sigs via resolveAnonymousTypeMembers/getDefaultConstructSignatures
- **TSR:** crates/tsr-checker/src/inference.rs object/signature inference (no construct-signature view of a class static type target)
- **Example:** `conformance/typeArgumentInferenceWithClassExpression3` [0:11] `length`. want `number`, got `any`
- **Example:** `conformance/typeArgumentInferenceWithClassExpression3` [0:5] `foo(class { prop = "hello" }).length`. want `number`, got `any`
- **Wave-1 evidence:** typeArgumentInferenceWithClassExpression3: foo(class { prop = "hello" }) want string got unknown; p19.ts: `f<T>(x: new () => {p:T})` infers string, `g<T>(x = class { p: T })` gives unknown
- **Cases:** conformance/typeArgumentInferenceWithClassExpression3

### T59. `OBJLIT-PATTERN-OPTIONAL` (1; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause / port plan:** An object literal contextually typed by a binding pattern's implied type marks properties optional where the pattern has a default; must work through ParenthesizedExpression.
- **tsgo:** internal/checker/checker.go checkObjectLiteral (implied-type optional flag) + getContextualType parenthesized arm
- **TSR:** crates/tsr-checker/src/binding_patterns.rs / contextual.rs (parenthesized initializer not given pattern context)
- **Example:** `compiler/classExpressionNames` [0:23] `({ B: undefined })`. want `{ B?: undefined; }`, got `{ B: undefined; }`
- **Example:** `compiler/classExpressionNames` [0:24] `{ B: undefined }`. want `{ B?: undefined; }`, got `{ B: undefined; }`
- **Wave-1 evidence:** classExpressionNames `({ B: undefined })` want { B?: undefined; } got { B: undefined; }; unparenthesized version right
- **Cases:** compiler/classExpressionNames

### T60. `REVERSE-MAPPED-TEMPLATE-INFERENCE` (1; blocked 1, finished alone 1, lines 2; confidence n/a)

- **Root cause / port plan:** 
- **tsgo:** 
- **TSR:** 
- **Example:** `compiler/contravariantOnlyInferenceFromAnnotatedFunction` [0:7] `result`. want `[string, { bar: string; }]`, got `error`
- **Example:** `compiler/contravariantOnlyInferenceFromAnnotatedFunction` [0:8] `foo({  bar: {    fn: (a: string) => {},    thing: 'asd',  },})`. want `[string, { bar: string; }]`, got `error`
- **Cases:** compiler/contravariantOnlyInferenceFromAnnotatedFunction

### T61. `ENUM-RELATION-SAME-NAME` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** isEnumTypeRelatedTo for two distinct enums with the same NAME requires every source member to exist in target with equal value; TSR leaves the same-name case undecided and subtype reduction collapses the union.
- **tsgo:** internal/checker/relater.go:282 isEnumTypeRelatedTo
- **TSR:** crates/tsr-checker/src/relater.rs ~L1646-1659 (same_name -> None, 'not ported')
- **Example:** `compiler/enumAssignmentCompat4` [0:18] `broken`. want `({ foo: M.MyEnum; } \| { foo: N.MyEnum; })[]`, got `{ foo: M.MyEnum; }[]`
- **Example:** `compiler/enumAssignmentCompat4` [0:19] `[    N.object1,    M.object2]`. want `({ foo: M.MyEnum; } \| { foo: N.MyEnum; })[]`, got `{ foo: M.MyEnum; }[]`
- **Wave-1 evidence:** enumAssignmentCompat4: `[N.object1, M.object2]` want `({ foo: M.MyEnum; } \| { foo: N.MyEnum; })[]` / got `{ foo: M.MyEnum; }[]`; probe `[a: M.E, b: N.E]` -> `M.E[]`
- **Cases:** compiler/enumAssignmentCompat4

### T62. `TUPLE-NORMALIZE-NAMED-VARIADIC` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** A tuple type node whose variadic element is a LABELLED member over a generic type (`[...args: T]`, `[...args: {..}[SS]]`) must normalise to a generic variadic tuple `[...args: T]` (createNormalizedTupleType keeps InstantiableNonPrimitive as Variadic); TSR yields `error[]` when the labelled variadic is the only element (unlabelled `[...T]` and `[x: string, ...args: X]` work).
- **tsgo:** internal/checker/checker.go:24115 getTypeFromArrayOrTupleTypeNode -> :23351 createNormalizedTupleType
- **TSR:** crates/tsr-checker/src/declared.rs get_type_from_tuple_type_node / tuple_type_node_structural (RestTypeNode(NamedTupleMember) variadic arm)
- **Example:** `compiler/genericTupleWithSimplifiableElements` [0:12] `w`. want `[...args: { [S in SS]: [a: number]; }[SS]]`, got `error[]`
- **Example:** `compiler/genericTupleWithSimplifiableElements` [0:13] `[1]`. want `[1]`, got `number[]`
- **Wave-1 evidence:** genericTupleWithSimplifiableElements: let w: [...args: {[S in SS]:[a:number]}[SS]] want same got error[]; probe `[...args: T]` -> error[] but `[...T]` right
- **Cases:** compiler/genericTupleWithSimplifiableElements [type-operators]

### T63. `INFERENCE-KEYOF-STRING-LITERAL` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** inferFromTypes arm: a string-literal (union) source inferred to a `keyof T` target creates an empty object type with those keys typed any (createEmptyObjectTypeFromStringLiteral) as a LiteralKeyof-priority candidate for T. TSR has the priority flag but no arm; the call errors.
- **tsgo:** internal/checker/inference.go:238 inferFromTypes (keyof target arm) -> :1229 createEmptyObjectTypeFromStringLiteral
- **TSR:** crates/tsr-checker/src/inference.rs infer_from_types (InferencePriority::LITERAL_KEYOF declared, arm missing)
- **Example:** `compiler/inferObjectTypeFromStringLiteralToKeyof` [0:6] `x`. want `{ a: any; d: any; }`, got `error`
- **Example:** `compiler/inferObjectTypeFromStringLiteralToKeyof` [0:7] `inference1(two)`. want `{ a: any; d: any; }`, got `error`
- **Wave-1 evidence:** inferObjectTypeFromStringLiteralToKeyof: inference1(two) want { a: any; d: any; } got error; probe inference1("a") -> error
- **Cases:** compiler/inferObjectTypeFromStringLiteralToKeyof

### T64. `GLOBAL-SCRIPT-INTERFACE-MERGE` (1; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause / port plan:** A script-level (non-module) `interface Generator<T>` merges with the lib's global Generator<T, TReturn = any, TNext = any>; references then carry the merged symbol's 3 type parameters with defaults filled (Generator<U, any, any>). TSR treats the local declaration alone.
- **tsgo:** internal/checker/checker.go mergeSymbolTable(globals) + :23169 getTypeFromClassOrInterfaceReference (fillMissingTypeArguments)
- **TSR:** crates/tsr-checker/src/declared.rs local_type_parameters_of / global merge of script declarations
- **Example:** `compiler/innerTypeArgumentInference` [0:1] `func`. want `Generator<U, any, any>`, got `Generator<U>`
- **Example:** `compiler/innerTypeArgumentInference` [0:4] `func`. want `Generator<U, any, any>`, got `Generator<U>`
- **Wave-1 evidence:** innerTypeArgumentInference: func: Generator<U> want Generator<U, any, any> got Generator<U>
- **Cases:** compiler/innerTypeArgumentInference [contextual]

### T65. `JSX-ELEMENT-TYPE-UNRESOLVED` (1; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause / port plan:** When JSX.Element cannot be resolved, getJsxElementTypeAt returns errorType — a real type that composes (array of it is any[]); TSR leaves a gap that poisons enclosing expressions.
- **tsgo:** internal/checker/jsx.go:1275 getJsxElementTypeAt / jsx.go:72 checkJsxElement
- **TSR:** crates/tsr-checker/src/jsx checking
- **Example:** `compiler/jsxFactoryIdentifier` [1:9] `view`. want `() => any[]`, got `() => any`
- **Example:** `compiler/jsxFactoryIdentifier` [1:10] `[`. want `any[]`, got `error`
- **Wave-1 evidence:** probe ae2: `[<meta></meta>]` tsgo any[], TSR error
- **Cases:** compiler/jsxFactoryIdentifier

### T66. `NARROW-TYPEOF-DISCRIMINANT-OPTCHAIN` (1; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause / port plan:** typeof x?.prop narrowing of a union declared as AliasInstantiation | null: optionalChainContainsReference + getDiscriminantPropertyAccess + narrowTypeByDiscriminant; TSR fails only for alias-instantiation|null via ?.
- **tsgo:** internal/checker/flow.go:614 narrowTypeByTypeof
- **TSR:** crates/tsr-checker/src/flow.rs typeof narrowing / discriminant access
- **Example:** `compiler/narrowingTypeofDiscriminant` [0:51] `wrapped`. want `{ value?: string; }`, got `WrappedStringOr<boolean>`
- **Example:** `compiler/narrowingTypeofDiscriminant` [0:63] `wrapped`. want `{ value?: string; }`, got `WrappedStringOr<boolean>`
- **Wave-1 evidence:** narrowingTypeofDiscriminant: wrapped want { value?: string; } / got WrappedStringOr<boolean>
- **Cases:** compiler/narrowingTypeofDiscriminant

### T67. `ACCESSOR-CIRCULAR-ANY` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** getTypeOfAccessors: a getter whose return body reads itself hits a resolution cycle and yields any (implicit-any error). TSR answers no type.
- **tsgo:** internal/checker/checker.go:18511 getTypeOfAccessors
- **TSR:** crates/tsr-checker/src/symbols.rs get_type_of_accessors (L219)
- **Example:** `compiler/recursiveGetterAccess` [0:1] `testProp`. want `any`, got `error`
- **Example:** `compiler/recursiveGetterAccess` [0:2] `this.testProp`. want `any`, got `error`
- **Wave-1 evidence:** recursiveGetterAccess `testProp` want any / got error; `this.testProp` downstream
- **Cases:** compiler/recursiveGetterAccess

### T68. `SATISFIES-ERROR-TARGET` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** checkSatisfiesExpression returns the target type itself when it is an error type (an unresolved `bleh` prints as `bleh`), otherwise the operand type. TSR always returns the operand type.
- **tsgo:** internal/checker/checker.go:10741 checkSatisfiesExpression (:10746-10748)
- **TSR:** crates/tsr-checker/src/expressions.rs Expression::SatisfiesExpression arm (L988, 'TRANSPARENT')
- **Example:** `compiler/satisfiesEmit` [0:1] `p`. want `bleh`, got `any`
- **Example:** `compiler/satisfiesEmit` [0:2] `a satisfies bleh`. want `bleh`, got `any`
- **Wave-1 evidence:** satisfiesEmit `a satisfies bleh` want `bleh` / got any; probe w.ts `z satisfies bleh` tsgo `bleh` / TSR number
- **Cases:** compiler/satisfiesEmit

### T69. `INFERENCE-UNION-TUPLE-TARGET` (1; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause / port plan:** Inferring from a tuple source to a union of tuple targets of different arity ([T] | [T, U]) via inferToMultipleTypes; uninferred U falls to unknown.
- **tsgo:** internal/checker/inference.go:448 inferToMultipleTypes, :699 inferFromObjectTypes
- **TSR:** crates/tsr-checker/src/inference.rs (union target inference)
- **Example:** `compiler/tupleTypeInference2` [0:18] `g([[]] as [void[]])`. want `unknown`, got `error`
- **Example:** `compiler/tupleTypeInference2` [0:26] `h([[]] as [void[]])`. want `unknown`, got `error`
- **Wave-1 evidence:** tupleTypeInference2 0:18 g([[]] as [void[]]): want unknown / got error; probed g2([1] as [number]) -> T also gaps
- **Cases:** compiler/tupleTypeInference2

### T70. `INFERENCE-MATCHING-UNION-IDENTITY` (1; blocked 1, finished alone 1, lines 2; confidence low)

- **Root cause / port plan:** inferFromTypes union-to-union step (inferFromMatchingTypes with identical/closely-matched constituents) must strip the shared `undefined` from T | undefined <- "admin" | undefined; fails in TSR only when the source union is a body-inferred return type (identity mismatch of constituents).
- **tsgo:** internal/checker/inference.go:65 inferFromTypes (union target arm)
- **TSR:** crates/tsr-checker/src/inference.rs union inference + signatures.rs get_return_type_from_body (union identity)
- **Example:** `compiler/typePredicateTopLevelTypeParameter` [0:24] `foundAdmins`. want `"admin"[]`, got `("admin" \| undefined)[]`
- **Example:** `compiler/typePredicateTopLevelTypeParameter` [0:25] `admins.filter(isDefined)`. want `"admin"[]`, got `("admin" \| undefined)[]`
- **Wave-1 evidence:** typePredicateTopLevelTypeParameter 0:24: want "admin"[] / got ("admin" \| undefined)[]; probed: declared return "admin"\|undefined works, inferred return fails (oracle confirms want)
- **Cases:** compiler/typePredicateTopLevelTypeParameter

### T71. `SPREAD-OPTIONAL-MISSING` (1; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause / port plan:** getSpreadType copies optional properties with their declared type plus missingType (printed without `| undefined` when exactOptionalPropertyTypes is off); TSR adds plain undefined.
- **tsgo:** internal/checker/checker.go:13387 getSpreadType, :13585 getSpreadSymbol
- **TSR:** crates/tsr-checker/src/spreads.rs get_spread_type
- **Example:** `compiler/unionExcessPropsWithPartialMember` [0:6] `ab = {...a, y: (null as any as string \| undefined)}`. want `{ unused?: string; x: string; y: string \| undefined; }`, got `{ unused?: string \| undefined; x: string; y: string \| undefined; }`
- **Example:** `compiler/unionExcessPropsWithPartialMember` [0:8] `{...a, y: (null as any as string \| undefined)}`. want `{ unused?: string; x: string; y: string \| undefined; }`, got `{ unused?: string \| undefined; x: string; y: string \| undefined; }`
- **Wave-1 evidence:** unionExcessPropsWithPartialMember 0:8: want { unused?: string; ... } / got { unused?: string \| undefined; ... }; probed {...a}
- **Cases:** compiler/unionExcessPropsWithPartialMember

### T72. `ARRAY-LITERAL-ERROR-ELEMENT` (1; blocked 1, finished alone 1, lines 2; confidence medium)

- **Root cause / port plan:** checkArrayLiteral with an errorType element: tsgo unions the element types (errorType) and still creates Array<errorType>, printed `any[]`. TSR's check_array_literal_value returns intrinsics.error as soon as an element is error (error doubling as its gap marker), so the literal and an initialised variable print `any` instead of `any[]`.
- **tsgo:** internal/checker/checker.go:8021 checkArrayLiteral (createArrayLiteralType(createArrayType(getUnionType(elementTypes))))
- **TSR:** crates/tsr-checker/src/array_literals.rs check_array_literal_value (L795-809 `if self.is_error(t) { return error }`)
- **Example:** `conformance/arbitraryModuleNamespaceIdentifiers_importEmpty` [0:3] `xyz`. want `any[]`, got `any`
- **Example:** `conformance/arbitraryModuleNamespaceIdentifiers_importEmpty` [0:4] `[x, y, z]`. want `any[]`, got `any`
- **Wave-1 evidence:** conformance/arbitraryModuleNamespaceIdentifiers_importEmpty: self-import of missing exports -> x,y,z errorType (lines print any, right); `[x, y, z]` want any[] got any. Minimised selfimp.ts: `[x]` error vs unresolved module case any[]
- **Cases:** conformance/arbitraryModuleNamespaceIdentifiers_importEmpty

### T73. `AUTO-TYPE-UNDEFINED-INIT` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** getTypeForVariableLikeDeclaration's autoType arm (noImplicitAny): a non-const variable whose initializer isNullOrUndefined — including the IDENTIFIER `undefined`, not just `null` — gets control-flow-tracked autoType, so a later reference narrows to `undefined`. TSR's auto road recognises a missing initializer and `null` but not `= undefined`, so the reference stays `any`.
- **tsgo:** internal/checker/checker.go:16697-16705 getTypeForVariableLikeDeclaration (autoType arm, c.isNullOrUndefined(initializer))
- **TSR:** crates/tsr-checker/src/flow.rs is_auto_typed_declaration (L2116) / symbols.rs autoType arm (~L5350)
- **Example:** `conformance/assignEveryTypeToAny` [0:31] `x = e`. want `undefined`, got `any`
- **Example:** `conformance/assignEveryTypeToAny` [0:33] `e`. want `undefined`, got `any`
- **Wave-1 evidence:** conformance/assignEveryTypeToAny: `var e = undefined; x = e;` want `e : undefined`, `x = e : undefined` got any; minimised `let a = null; const b = a` -> null (ok) but `var c = undefined; const d = c` -> any
- **Cases:** conformance/assignEveryTypeToAny

### T74. `INDEX-SIG-INVALID-OR-UNTYPED` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** Index signatures with an invalid key type are dropped (isValidIndexKeyType) and a missing value annotation means any; the type literal still resolves.
- **tsgo:** internal/checker/checker.go:19634 getIndexInfosOfIndexSymbol (19650-19658)
- **TSR:** crates/tsr-checker/src/index_signatures.rs:375 index_infos_of_symbol / declared.rs type literal
- **Example:** `conformance/parserIndexSignature8` [0:0] `foo`. want `{}`, got `any`
- **Example:** `conformance/parserIndexSignature8` [0:2] `foo2`. want `{}`, got `any`
- **Wave-1 evidence:** parserIndexSignature8: { [index: any]; } want {} got any; probe { [index: string]; } -> error and { [index:any]: number } keeps an any-keyed sig
- **Cases:** conformance/parserIndexSignature8

### T75. `INFERENCE-MATCHING-INTERSECTION-CONSTITUENTS` (1; blocked 1, finished alone 1, lines 2; confidence high)

- **Root cause / port plan:** inferFromTypes with an intersection target and intersection source: constituents identical in source and target are matched and removed (inferFromMatchingTypes with isTypeIdenticalTo) before inferring the remaining source to the naked type parameter.
- **tsgo:** internal/checker/inference.go:73 inferFromTypes (intersection arm) -> inference.go:369 inferFromMatchingTypes
- **TSR:** crates/tsr-checker/src/inference.rs infer_from_types (intersection target)
- **Example:** `conformance/unionAndIntersectionInference2` [0:35] `f2(a2)`. want `string`, got `string & { name: string; }`
- **Example:** `conformance/unionAndIntersectionInference2` [0:38] `f2(b2)`. want `string[]`, got `{ name: string; } & string[]`
- **Wave-1 evidence:** unionAndIntersectionInference2: f2(a2) a2: string & { name: string } want string got string & { name: string; }
- **Cases:** conformance/unionAndIntersectionInference2

### T76. `QUICK-TYPE-OF-EXPRESSION` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** Writer's getTypeOfNode->getRegularTypeOfExpression->getTypeOfExpression takes the getQuickTypeOfExpression fast path: a call/new with a single non-generic signature answers that signature's return type without resolution (so `new AbstractClass()` prints the instance type, not errorType).
- **tsgo:** internal/checker/checker.go:7361 getQuickTypeOfExpression (via 7337 getTypeOfExpression, 32111 getRegularTypeOfExpression)
- **TSR:** crates/tsr-conformance/src/types_producer.rs:1577 type_id_at_location_tracking calls checker.check_expression (no quick path)
- **Example:** `compiler/abstractClassInLocalScopeIsAbstract` [0:6] `new A()`. want `A`, got `any`
- **Wave-1 evidence:** abstractClassInLocalScopeIsAbstract: want `new A() : A` / got error; resolveNewExpression returns resolveErrorCall for abstract, quick path bypasses it
- **Cases:** compiler/abstractClassInLocalScopeIsAbstract

### T77. `UNION-TUPLE-INDEX-INFO` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** Index infos of a union whose constituents are tuples (number index from Array<E>) via getUnionIndexInfos.
- **tsgo:** internal/checker/checker.go:13510 getUnionIndexInfos
- **TSR:** crates/tsr-checker/src/index_signatures.rs:302 union_index_infos (tuple constituents yield None)
- **Example:** `compiler/avoidNarrowingUsingConstVariableFromBindingElementWithLiteralInitializer` [0:8] `foo[index]`. want `string \| number \| boolean`, got `error`
- **Wave-1 evidence:** avoidNarrowing...: foo[index] want string\|number\|boolean got error; `[string]\|[number]` [i] errors; single tuple OK
- **Cases:** compiler/avoidNarrowingUsingConstVariableFromBindingElementWithLiteralInitializer

### T78. `ACCESSOR-WRITE-TYPE-ELEMENT-ACCESS` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** Element access used as assignment target reads the property's WRITE type (set accessor parameter) for computed/unique-symbol keys.
- **tsgo:** internal/checker/checker.go getIndexedAccessType with AccessFlagsWriting -> getWriteTypeOfSymbol
- **TSR:** crates/tsr-checker/src/indexed.rs (read type used)
- **Example:** `compiler/computedPropertiesWithSetterAssignment` [0:36] `foo[k]`. want `Iterable<string>`, got `Set<string>`
- **Wave-1 evidence:** computedPropertiesWithSetterAssignment `foo[k]` want Iterable<string> got Set<string>. Not minimised
- **Cases:** compiler/computedPropertiesWithSetterAssignment

### T79. `WITH-STATEMENT-BODY-ERROR` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** getTypeOfNode returns errorType for any node with NodeFlagsInWithStatement (inside a `with` body), including declaration names.
- **tsgo:** internal/checker/checker.go:31932 getTypeOfNode (NodeFlagsInWithStatement -> errorType)
- **TSR:** crates/tsr-conformance/src/types_producer.rs / crates/tsr-checker/src/check.rs in-with-statement predicate (~L13405) not applied to declaration-name lines
- **Example:** `compiler/elidedEmbeddedStatementsReplacedWithSemicolon` [0:16] `H`. want `error`, got `H`
- **Wave-1 evidence:** elidedEmbeddedStatementsReplacedWithSemicolon: `with (window) const enum H {}` H want error / got H
- **Cases:** compiler/elidedEmbeddedStatementsReplacedWithSemicolon

### T80. `NARROWABLE-REF-BINDING-PATTERN-CONTEXT` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** getNarrowableTypeForReference substitutes a generic reference's union constraint when hasContextualTypeWithNoGenericTypes holds; for the initializer of a destructuring declaration the contextual type is getTypeFromBindingPattern (non-generic), so `= params` (params: P extends union-constrained) prints the constraint. TSR's get_contextual_type yields no contextual type for a binding-pattern initializer, so the reference keeps P.
- **tsgo:** internal/checker/checker.go:31491 getNarrowableTypeForReference / :31533 hasContextualTypeWithNoGenericTypes (getContextualTypeForInitializerExpression binding-pattern arm)
- **TSR:** crates/tsr-checker/src/constraints.rs narrowable_type_for_reference (concrete_context via get_contextual_type) + contextual.rs initializer-of-binding-pattern contextual type
- **Example:** `compiler/genericObjectSpreadResultInSwitch` [0:11] `params`. want `Params`, got `P`
- **Wave-1 evidence:** genericObjectSpreadResultInSwitch: `const { foo, ...rest } = params` want Params got P; oracle probe Q extends U1: `const {tag,...r2} = q` tsgo q: U1, TSR Q while take(q) argument is right
- **Cases:** compiler/genericObjectSpreadResultInSwitch

### T81. `CONDITIONAL-INFER-TO-MAPPED` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** Conditional-type inference (inferTypes for `infer U`) into a mapped-type extends target `{ [_ in keyof T]: infer U }`: inferToMappedType infers U from the union of the source's property (and index) types. TSR errors the whole conditional.
- **tsgo:** internal/checker/inference.go:948 inferToMappedType (via getConditionalType inferTypes)
- **TSR:** crates/tsr-checker/src/inference.rs mapped-target inference (infer_to_mapped_type equivalent) / declared.rs evaluate_conditional_alias
- **Example:** `compiler/inferConditionalConstraintMappedMember` [0:1] `test`. want `never`, got `error`
- **Wave-1 evidence:** inferConditionalConstraintMappedMember: type test = KeysWithoutStringIndex<{...}> want never got error; oracle probe V<T> = T extends {[_ in keyof T]: infer U} ? U : never, V<{foo:string;bar:'baz'}> tsgo string, TSR error
- **Cases:** compiler/inferConditionalConstraintMappedMember

### T82. `INFER-TYPEPARAM-IMPLIED-CONSTRAINT` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** An `infer B` placed as a type argument of a generic alias/reference (`SubGuard<M[number], infer B>` with `X extends [A]`) gets an implied constraint from that type parameter's constraint (getInferredTypeParameterConstraint), and the inferred result is constrained to it ([1|2|3|4]). TSR errors.
- **tsgo:** internal/checker/checker.go:17114 getInferredTypeParameterConstraint (via :17071 getConstraintFromTypeParameter)
- **TSR:** crates/tsr-checker/src/declared.rs conditional evaluation of infer type parameters (no implied-constraint port)
- **Example:** `compiler/inferTypeParameterConstraints` [0:2] `E0`. want `[1 \| 2 \| 3 \| 4]`, got `error`
- **Wave-1 evidence:** inferTypeParameterConstraints: type E0 = IsSub<[1,2,3,4],[2,3,4]> want [1 \| 2 \| 3 \| 4] got error; oracle probe: same without SubGuard wrapper -> unknown[] in both
- **Cases:** compiler/inferTypeParameterConstraints

### T83. `NARROWABLE-REF-CONTEXTUAL` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** getNarrowableTypeForReference substitutes union-constraint of a generic reference when hasContextualTypeWithNoGenericTypes; for a call argument after resolution the contextual type is the instantiated (non-generic) parameter type.
- **tsgo:** internal/checker/checker.go:31491 getNarrowableTypeForReference / 31533 hasContextualTypeWithNoGenericTypes
- **TSR:** crates/tsr-checker/src/constraints.rs:240 narrowable_type_for_reference (get_contextual_type sees generic/uninstantiated param)
- **Example:** `compiler/intersectionSatisfiesConstraint` [0:17] `newParam`. want `(FirstInterface \| SecondInterface) & { otherProperty: number; }`, got `T & { otherProperty: number; }`
- **Wave-1 evidence:** intersectionSatisfiesConstraint: want `(FirstInterface \| SecondInterface) & {...}` got `T & {...}` (oracle probe e2.ts)
- **Cases:** compiler/intersectionSatisfiesConstraint

### T84. `KEYOF-UNIQUE-SYMBOL-KEYS` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** keyof includes unique-symbol (late-bound) property keys (getLiteralTypeFromProperties include ESSymbolLike).
- **tsgo:** internal/checker/checker.go:26717 getLiteralTypeFromProperties / 26680 getIndexType
- **TSR:** crates/tsr-checker/src/keyof evaluation
- **Example:** `compiler/keyofObjectWithGlobalSymbolIncluded` [0:7] `Q`. want `unique symbol`, got `never`
- **Wave-1 evidence:** probe ko2: `keyof {[s]:1,a:1}` TSR "a", tsgo "a" \| unique symbol
- **Cases:** compiler/keyofObjectWithGlobalSymbolIncluded

### T85. `INFERENCE-MAPPED-ENUM-OBJECT-SOURCE` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** inferToMappedType (constraint is a type param K): V inferred from the union of the source's property types; for an enum object source this is the enum type.
- **tsgo:** internal/checker/inference.go:948 inferToMappedType
- **TSR:** crates/tsr-checker/src/inference.rs mapped-type inference (enum object properties)
- **Example:** `compiler/mappedToToIndexSignatureInference` [0:15] `enumValues(E)`. want `E[]`, got `never[]`
- **Wave-1 evidence:** probe mi3: `m2<K,V>(e:{[P in K]:V}); m2(E)` TSR never, object of enum literals -> E fine
- **Cases:** compiler/mappedToToIndexSignatureInference

### T86. `NARROWED-TYPE-UNKNOWN-FALSE-BRANCH` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** false branch of a type predicate on unknown: getNarrowedTypeWorker expands to unknownUnionType and filters constituents that are subsets of the true type -> null | undefined; TSR returns unknown
- **tsgo:** internal/checker/flow.go:859 getNarrowedTypeWorker
- **TSR:** crates/tsr-checker/src/flow.rs narrowed_type_worker / narrowed_constituent
- **Example:** `compiler/narrowUnknownByTypePredicate` [0:14] `value2`. want `null \| undefined`, got `unknown`
- **Wave-1 evidence:** narrowUnknownByTypePredicate: !isNotNullish(value2) want null \| undefined / got unknown
- **Cases:** compiler/narrowUnknownByTypePredicate

### T87. `TYPE-LITERAL-COMPUTED-ACCESSOR` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** type literal containing a get/set accessor with a computed (non-literal) name still yields an object type; TSR declared type is error
- **tsgo:** internal/checker/checker.go:20894-region resolveAnonymousTypeMembers / getMembersOfSymbol (late-bound names)
- **TSR:** crates/tsr-checker/src/declared.rs get_type_from_type_literal accessor members
- **Example:** `compiler/noMappedGetSet` [0:0] `OH_NO`. want `OH_NO`, got `any`
- **Wave-1 evidence:** noMappedGetSet: OH_NO want OH_NO / got any; oracle `get [x](): string` same
- **Cases:** compiler/noMappedGetSet

### T88. `MERGE-SYMBOL-CONFLICT-NO-MERGE` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** mergeSymbol with excluded flags (type alias into interface+namespace) reports an error and does NOT merge/record, so the alias keeps its own symbol
- **tsgo:** internal/checker/checker.go:14146 mergeSymbol
- **TSR:** crates/tsr-binder (global merge) / checker merged_symbol
- **Example:** `compiler/noSymbolForMergeCrash` [1:0] `A`. want `{}`, got `A`
- **Wave-1 evidence:** noSymbolForMergeCrash: A want {} / got A
- **Cases:** compiler/noSymbolForMergeCrash

### T89. `ALIAS-CIRCULAR-ANY` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** resolveAlias on an import=/export= cycle returns unknownSymbol after reporting circularity; getTypeOfAlias yields errorType which the writer prints as `any`. TSR has no type for the alias.
- **tsgo:** internal/checker/checker.go:16266 resolveAlias; :18598 getTypeOfAlias
- **TSR:** crates/tsr-checker/src/symbols.rs get_type_of_alias (L409) / resolve_alias (L943)
- **Example:** `compiler/recursiveExportAssignmentAndFindAliasedType7` [1:2] `self`. want `any`, got `error`
- **Wave-1 evidence:** recursiveExportAssignmentAndFindAliasedType7 [1:2] `self` want any / got error (C→D→E→C export= chain)
- **Cases:** compiler/recursiveExportAssignmentAndFindAliasedType7

### T90. `REVERSE-MAPPED-PROPERTY-ORDER` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** Reverse-mapped type members follow getPropertiesOfType(source) declaration order. TSR emits them reversed when the source is a NAMED type (alias of type literal or interface); inline literal sources are right.
- **tsgo:** internal/checker/inference.go:1099 resolveReverseMappedTypeMembers; :1014 createReverseMappedType
- **TSR:** crates/tsr-checker/src/inference.rs reverse_mapped_member_plan (L3456; property_names_of(source) order)
- **Example:** `compiler/reverseMappedTypeAssignableToIndex` [0:11] `Inferred`. want `{ first: "first"; second: "second"; }`, got `{ second: "second"; first: "first"; }`
- **Wave-1 evidence:** reverseMappedTypeAssignableToIndex `Inferred` want `{ first: "first"; second: "second"; }` / got `{ second: ...; first: ... }`; probe u4.ts named vs inline
- **Cases:** compiler/reverseMappedTypeAssignableToIndex

### T91. `REST-TUPLE-ALIAS-NORMALIZE` (1; blocked 1, finished alone 1, lines 1; confidence n/a)

- **Root cause / port plan:** 
- **tsgo:** 
- **TSR:** 
- **Example:** `compiler/singletonLabeledTuple` [0:17] `AliasedRest`. want `false`, got `AliasRest extends [unknown] ? true : false`
- **Cases:** compiler/singletonLabeledTuple [type-operators]

### T92. `BINARY-PLUS-CONSTRAINT` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** `+`/`+=` arm of checkBinaryLikeExpression classifies operands by isTypeAssignableToKind on base constraints; T[K] with T extends Record<K, number> is NumberLike -> number.
- **tsgo:** internal/checker/checker.go:12414 checkBinaryLikeExpression (PlusToken arm)
- **TSR:** crates/tsr-checker/src/binary.rs check_binary_expression
- **Example:** `compiler/typeParameterExtendsPrimitive` [0:30] `result += v[prop]`. want `number`, got `any`
- **Wave-1 evidence:** typeParameterExtendsPrimitive 0:30 result += v[prop]: want number / got any; probed `r + v[prop]` gaps while `v[prop] * 2` works
- **Cases:** compiler/typeParameterExtendsPrimitive

### T93. `ASYNC-ACCESSOR-FUNCTION-FLAGS` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** ast.GetFunctionFlags only reads `async`/`*` for function declarations/expressions, methods and arrows — an (invalid) `async` modifier on a get accessor is ignored, so getReturnTypeFromBody infers the plain body return (`void`). TSR treats the async getter body as async and declines (gap).
- **tsgo:** internal/ast/functionflags.go:13 GetFunctionFlags; internal/checker/checker.go:20126 getReturnTypeFromBody via :18511 getTypeOfAccessors
- **TSR:** crates/tsr-checker/src/symbols.rs get_type_of_accessors_worker (L287) -> get_return_type_from_body async arm
- **Example:** `conformance/asyncGetter_es6` [0:1] `foo`. want `void`, got `any`
- **Wave-1 evidence:** conformance/asyncGetter_es6: `async get foo() { }` want `foo : void` got any/error; minimised ag.ts: plain getter void, async getter error
- **Cases:** conformance/asyncGetter_es6

### T94. `PRIVATE-IDENTIFIER-EXPRESSION` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** checkPrivateIdentifierExpression: a bare `#x` used as the left operand of `#x in obj` always types as anyType (after resolving the symbol for reference marking). TSR's check_expression has no PrivateIdentifier arm and answers error (gap).
- **tsgo:** internal/checker/checker.go:7837 checkPrivateIdentifierExpression
- **TSR:** crates/tsr-checker/src/expressions.rs check_expression (no Expression::PrivateIdentifier arm)
- **Example:** `conformance/autoAccessor10` [0:15] `#a2_accessor_storage`. want `any`, got `error`
- **Wave-1 evidence:** conformance/autoAccessor10: `#a2_accessor_storage in C3` want `#a2_accessor_storage : any` got error; minimised `#x in o` -> `#x : error`
- **Cases:** conformance/autoAccessor10

### T95. `ENUM-EVAL-TEMPLATE` (1; blocked 1, finished alone 1, lines 1; confidence high)

- **Root cause / port plan:** Enum member initializer that is a template expression WITH substitutions (`1${"2"}3`): upstream's constant evaluator folds TemplateExpression (head + evaluated spans) to the string "123", and getEnumLiteralType reuses the first same-valued member's type (T5.c). TSR's sequential enum folder only handles string/no-substitution-template literals, so the member is computed and prints its own name.
- **tsgo:** internal/evaluator/evaluator.go:108 evaluate (KindTemplateExpression arm) via internal/checker/checker.go:23938 computeEnumMemberValues
- **TSR:** crates/tsr-checker/src/declared.rs get_declared_type_of_enum (sequential constant folder, ~L5552 handles NoSubstitutionTemplateLiteral only)
- **Example:** `conformance/enumConstantMemberWithTemplateLiterals` [0:59] `g`. want `T5.c`, got `T5.g`
- **Wave-1 evidence:** enumConstantMemberWithTemplateLiterals: enum T5 { c = `1`+`2`+`3`, g = `1${"2"}3` } g want T5.c / got T5.g; probe aa.ts: `123` and "1"+"2"+"3" dedupe to T5.c, templated one does not
- **Cases:** conformance/enumConstantMemberWithTemplateLiterals

### T96. `IDENTIFIER-ASSIGN-NONVARIABLE` (1; blocked 1, finished alone 1, lines 1; confidence medium)

- **Root cause / port plan:** checkIdentifier on an assignment target whose symbol is not a variable (undefined, enum, class, function...) reports and returns errorType.
- **tsgo:** internal/checker/checker.go:11077 checkIdentifier (assignment arm)
- **TSR:** crates/tsr-checker/src/expressions.rs identifier checking (no non-variable assignment-target arm)
- **Example:** `conformance/nullAssignedToUndefined` [0:2] `undefined`. want `any`, got `undefined`
- **Wave-1 evidence:** nullAssignedToUndefined: `undefined = null` left operand want any got undefined
- **Cases:** conformance/nullAssignedToUndefined

### T97. `MERGED-FUNCTION-INTERFACE` (1; blocked 3, finished alone 0, lines 15; confidence high)

- **Root cause / port plan:** A symbol merging a `declare function Tag` with an `interface Tag<...>` (SymbolFlags FUNCTION|INTERFACE): getTypeOfSymbol takes the getTypeOfFuncClassEnumModule arm for the value and getDeclaredTypeOfSymbol the class/interface arm for the type; TSR answers error for both the function's value type and the generic interface reference `Tag<I, S>` (non-generic merge also errors on the function)
- **tsgo:** internal/checker/checker.go:16904 getTypeOfFuncClassEnumModule / :23169 getTypeFromClassOrInterfaceReference
- **TSR:** crates/tsr-checker/src/symbols.rs get_type_of_func_class_enum_module(_worker) + declared.rs generic reference to a merged value+type symbol
- **Example:** `compiler/contextualParamTypeVsNestedReturnTypeInference2` [0:19] `tag`. want `Tag<I, S>`, got `error`
- **Example:** `compiler/contextualParamTypeVsNestedReturnTypeInference2` [0:21] `Tag`. want `<const Id extends string>(id: Id) => <Self, Shape>() => TagClass<Self, Id, Shape>`, got `error`
- **Wave-1 evidence:** compiler/contextualParamTypeVsNestedReturnTypeInference2: `tag: Tag<I, S>` and `Tag` function both error; probe `interface Tag<Id>{..}; declare function Tag<...>(id: Id): Id;` -> `Tag : error`, `t: Tag<string>` error; renaming the interface fixes these lines
- **Cases:** compiler/contextualParamTypeVsNestedReturnTypeInference2 (+CONDITIONAL-INLINE-NODE-INSTANTIATION); compiler/contextualParamTypeVsNestedReturnTypeInference3 (+CONDITIONAL-INLINE-NODE-INSTANTIATION); compiler/contextualParamTypeVsNestedReturnTypeInference4 (+INFER-NO-CANDIDATE-GUARD)

### T98. `RELATE-CONDITIONAL` (1; blocked 2, finished alone 0, lines 3; confidence medium)

- **Root cause / port plan:** TSR's relater has no conditional-type arms of structuredTypeRelatedToWorker: (a) target conditional: related only if no infer params & not distribution-dependent (both branches); (b) source conditional vs target conditional with identical extends/check types relates branch-wise. Missing arms make TSR answer related where tsgo says no (narrowing picks candidate) or undecidable/error where tsgo relates (Equals<A,B> generic-signature trick).
- **tsgo:** internal/checker/relater.go:3540 (target Conditional arm) and :3721 (source Conditional arm) in structuredTypeRelatedToWorker (:3261)
- **TSR:** crates/tsr-checker/src/relater.rs (no conditional arm; relate_ternary) — consumers flow.rs narrowed_type_worker, declared.rs evaluate_conditional_alias
- **Example:** `compiler/genericConditionalConstrainedToUnknownNotAssignableToConcreteObject` [0:16] `a2`. want `ReturnType<T[M]> & A`, got `A`
- **Example:** `compiler/identityRelationNeverTypes` [0:24] `T1`. want `true`, got `error`
- **Wave-1 evidence:** genericConditionalConstrainedToUnknownNotAssignableToConcreteObject: isA narrowing of ReturnType<T[M]> want ReturnType<T[M]> & A got A; identityRelationNeverTypes: type T2 = Equals<never,never> want true got error; oracle probe Equals<1,1> tsgo true, TSR error
- **Cases:** compiler/genericConditionalConstrainedToUnknownNotAssignableToConcreteObject; compiler/identityRelationNeverTypes (+NODEBUILDER-GET-REDUCED-TYPE)

### T99. `RETURN-WIDEN-UNIQUE-SYMBOL` (1; blocked 2, finished alone 0, lines 2; confidence high)

- **Root cause / port plan:** Inferred return type of a function widens a unique symbol (getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded -> getWidenedUniqueESSymbolType) to symbol.
- **tsgo:** internal/checker/checker.go:20407 getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded, :25505 getWidenedUniqueESSymbolType
- **TSR:** crates/tsr-checker/src/signatures.rs get_return_type_from_body
- **Example:** `compiler/typeNamedUndefined1` [0:9] `x`. want `(p: ns.undefined) => symbol`, got `(p: ns.undefined) => ns.undefined`
- **Example:** `compiler/typeNamedUndefined2` [0:10] `x`. want `(p: ns.undefined.undefined) => symbol`, got `(p: ns.undefined.undefined) => ns.undefined.undefined`
- **Wave-1 evidence:** typeNamedUndefined1 0:9 want (p: ns.undefined) => symbol; probed `function g(){ return s }` -> () => unique symbol (oracle: symbol)
- **Cases:** compiler/typeNamedUndefined1 (+QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE); compiler/typeNamedUndefined2 (+IDENT-UNDEFINED-SHADOWED, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE)

### T100. `CONTEXTUAL-COMPUTED-SYMBOL-KEY` (1; blocked 1, finished alone 0, lines 20; confidence low)

- **Root cause / port plan:** getContextualTypeForObjectLiteralElement with a computed (unique symbol / late-bound) name passes the computed name type to getTypeOfPropertyOfContextualTypeEx, which for a generic mapped contextual type `{ [K in T['type']]: (p: K) => void }` substitutes K := typeof A; TSR loses the contextual type for computed-key members (arrow error, param any)
- **tsgo:** internal/checker/checker.go:29920 getContextualTypeForObjectLiteralElement -> :30555 getTypeOfPropertyOfContextualTypeEx (nameType / getIndexedMappedTypeSubstitutedTypeOfContextualType)
- **TSR:** crates/tsr-checker/src/contextual.rs contextual_type_for_object_literal_element (~L2125, computed names not resolved)
- **Example:** `compiler/contextuallyTypedSymbolNamedProperties` [0:24] `{    [A]: ap => { ap.description },    [B]: bp => { bp.description },}`. want `{ [A]: (ap: unique symbol) => void; [B]: (bp: unique symbol) => void; }`, got `error`
- **Example:** `compiler/contextuallyTypedSymbolNamedProperties` [0:25] `[A]`. want `(ap: unique symbol) => void`, got `error`
- **Wave-1 evidence:** compiler/contextuallyTypedSymbolNamedProperties: `{ [A]: ap => ... }` against `{ [K in T['type']]: (p: K) => void }` want ap: unique symbol got any (unverified by minimal probe)
- **Cases:** compiler/contextuallyTypedSymbolNamedProperties (+SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### T101. `NOINFER-SUBSTITUTION` (1; blocked 1, finished alone 0, lines 19; confidence high)

- **Root cause / port plan:** NoInfer<T> is an intrinsic alias creating a substitution type (getNoInferType); references strip it (getNarrowableTypeForReference), apparent type/property access and narrowing go through baseType. TSR has no NoInfer handling at all
- **tsgo:** internal/checker/checker.go:27394 getNoInferType; :31491 getNarrowableTypeForReference (isNoInferType strip)
- **TSR:** crates/tsr-checker/src/(absent: no NoInfer reference in crates/tsr-checker/src)
- **Example:** `compiler/narrowingNoInfer1` [0:17] `something`. want `({ result: TaggedA; } \| null)[]`, got `any[]`
- **Example:** `compiler/narrowingNoInfer1` [0:18] `map(m, (_) =>  _.result._tag === "a" ? { ..._, result: _.result }`. want `null,) : ({ result: TaggedA; } \| null)[]`, got `null,) : any[]`
- **Wave-1 evidence:** narrowingNoInfer1: _ want { result: NoInfer<TaggedUnion>; } / got NoInfer<{...}>; probe z: NoInfer<{type:'a'}> -> z.type error
- **Cases:** compiler/narrowingNoInfer1 (+OBJECT-LITERAL-PSEUDOTYPE-REUSE)

### T102. `INTERSECTION-TYPEVAR-CONSTRAINT-REDUCTION` (1; blocked 1, finished alone 0, lines 19; confidence high)

- **Root cause / port plan:** getIntersectionTypeEx reduces T & P (T type variable, P primitive/literal-union/{}) using T's constraint: to T when constraint is a strict subtype of P, to never when unrelated.
- **tsgo:** internal/checker/checker.go:26130 getIntersectionTypeEx (constraint reduction block)
- **TSR:** crates/tsr-checker/src/intersections.rs get_intersection_type
- **Example:** `compiler/typeVariableConstraintIntersections` [0:1] `T01`. want `never`, got `T01<K>`
- **Example:** `compiler/typeVariableConstraintIntersections` [0:10] `T30`. want `K`, got `T30<K>`
- **Wave-1 evidence:** typeVariableConstraintIntersections 0:1 T01 = K & "c" (K extends "a"\|"b"): want never / got T01<K>; T02 K & string want K
- **Cases:** compiler/typeVariableConstraintIntersections (+NODEBUILDER-GET-REDUCED-TYPE)

### T103. `APPARENT-MAPPED-ARRAY-CONSTRAINT` (1; blocked 1, finished alone 0, lines 15; confidence medium)

- **Root cause / port plan:** Property access on a generic homomorphic mapped type whose type parameter is constrained to an array. Upstream's getResolvedApparentTypeOfMappedType maps over the constraint, so `.map` is Array<PromiseSettledResult<unknown>>.map; TSR returns the template at key 'map'. Downstream, the callback parameter loses its contextual type and the tuple returns become arrays.
- **tsgo:** internal/checker/checker.go getApparentType -> getResolvedApparentTypeOfMappedType
- **TSR:** crates/tsr-checker/src/members.rs (apparent type / property lookup of mapped types)
- **Example:** `conformance/dependentDestructuredVariablesFromNestedPatterns` [0:23] `promises.map((result) =>    result.status === "fulfilled"      ? [result.value, undefined]     `. want `[undefined, new Error(String(result.reason))],  ) : ([unknown, undefined] \| [undefined, Error])[]`, got `[undefined, new Error(String(result.reason))],  ) : error`
- **Example:** `conformance/dependentDestructuredVariablesFromNestedPatterns` [0:24] `promises.map`. want `<U>(callbackfn: (value: PromiseSettledResult<unknown>, index: number, array: PromiseSettledResult<unknown>[]) => U, thisArg?: any) => U[]`, got `PromiseSettledResult<Awaited<T["map"]>>`
- **Wave-1 evidence:** dependentDestructuredVariablesFromNestedPatterns: promises.map want <U>(callbackfn: ...) => U[] / got PromiseSettledResult<Awaited<T["map"]>>
- **Cases:** conformance/dependentDestructuredVariablesFromNestedPatterns (+DESTRUCTURE-FLOW-DEPENDENT, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS, UNATTRIBUTED)

### T104. `FLOW-NONNULL-DECLARED-FALLBACK` (1; blocked 1, finished alone 0, lines 15; confidence high)

- **Root cause / port plan:** getFlowTypeOfReferenceEx: when the reference is the operand of `x!` and the flow type is non-never but entirely null/undefined (getTypeWithFacts NEUndefinedOrNull is never), the declared type is returned instead.
- **tsgo:** internal/checker/flow.go:111 getFlowTypeOfReferenceEx
- **TSR:** crates/tsr-checker/src/flow.rs get_flow_type_of_reference_ex (~L394)
- **Example:** `conformance/typeGuardsAsAssertions` [0:145] `x!.slice()`. want `string`, got `any`
- **Example:** `conformance/typeGuardsAsAssertions` [0:146] `x!.slice`. want `(start?: number, end?: number) => string`, got `any`
- **Wave-1 evidence:** typeGuardsAsAssertions f6: `x = undefined; x!.slice()` want x: string \| null \| undefined, x!: string; got undefined / never / slice error->any
- **Cases:** conformance/typeGuardsAsAssertions (+FLOW-ASSIGNMENT-REDUCED-GENERIC, FLOW-LOOP-INCOMPLETE-TYPES)

### T105. `PATTERN-AMBIENT-MODULE` (1; blocked 1, finished alone 0, lines 13; confidence high)

- **Root cause / port plan:** resolveExternalModule's pattern-ambient-module arm: an import of `"foobarbaz"` matches `declare module "foo*baz"` via FindBestPatternMatch (longest prefix) and resolves to that module's symbol; imported names then type from its exports. TSR's checker has no pattern-ambient resolution (only a diagnostic gate has_pattern_ambient_module), so the import aliases resolve to nothing (decl lines any, references gap).
- **tsgo:** internal/checker/checker.go:15149 resolveExternalModule (:15364-15366 patternAmbientModules / core.FindBestPatternMatch)
- **TSR:** crates/tsr-checker/src (module resolution for imports: no pattern arm; check.rs has_pattern_ambient_module L13380 is diagnostics-only); tsr-module util.rs find_best_pattern_match exists but unused by the checker
- **Example:** `conformance/ambientDeclarationsPatterns` [0:0] `foo`. want `(s: string) => void`, got `any`
- **Example:** `conformance/ambientDeclarationsPatterns` [0:1] `baz`. want `string`, got `any`
- **Wave-1 evidence:** conformance/ambientDeclarationsPatterns: `import {foo, baz} from "foobarbaz"` want `foo : (s: string) => void` got any; `foo(baz)` gap; minimised single-file probe `import {foo} from "foobarbaz"` -> any
- **Cases:** conformance/ambientDeclarationsPatterns (+AMBIENT-MODULE-SPECIFIER-MULTIDECL)

### T106. `OBJLIT-ACCESSOR-NONIDENT-NAME` (1; blocked 1, finished alone 0, lines 12; confidence high)

- **Root cause / port plan:** Object-literal get/set accessors with string/numeric literal names (paired via normalized symbol names) must be typed like identifier-named ones.
- **tsgo:** internal/checker/checker.go:13144 checkObjectLiteral accessor members (binder symbol naming via ast.GetPropertyNameForPropertyNameNode utilities.go:3160)
- **TSR:** crates/tsr-checker/src/objects.rs:1353 setter arm (only PropertyName::Identifier; else error)
- **Example:** `conformance/objectLiteralGettersAndSetters` [0:0] `sameName1a`. want `{ a: string; }`, got `error`
- **Example:** `conformance/objectLiteralGettersAndSetters` [0:1] `{ get 'a'() { return ''; }, set a(n) { var p = n; var p: string; } }`. want `{ a: string; }`, got `error`
- **Wave-1 evidence:** objectLiteralGettersAndSetters: { get 'a'(){return ''}, set a(n){} } want { a: string; } got error; identifier-named pair works
- **Cases:** conformance/objectLiteralGettersAndSetters (+GETTER-RETURN-ANNOTATION-CONTEXT, OBJECT-LITERAL-PSEUDOTYPE-REUSE)

### T107. `UNION-PROP-OBJECT-LITERAL-MISSING` (1; blocked 1, finished alone 0, lines 10; confidence medium)

- **Root cause / port plan:** Reading a property from a union where an object-literal member lacks it yields an optional/undefined-typed union property.
- **tsgo:** internal/checker/checker.go createUnionOrIntersectionProperty (object-literal missing-prop arm)
- **TSR:** crates/tsr-checker/src/members.rs union property lookup
- **Example:** `compiler/destructuringAssignmentWithDefault` [0:67] `color`. want `string \| undefined`, got `error`
- **Example:** `compiler/destructuringAssignmentWithDefault` [0:68] `width`. want `number \| undefined`, got `error`
- **Wave-1 evidence:** destructuringAssignmentWithDefault: (options \|\| {}).color want string \| undefined / got error
- **Cases:** compiler/destructuringAssignmentWithDefault (+REMOVE-SUBTYPES-UNDECIDABLE, TUPLE-OPTIONAL-ELEMENT-UNDEFINED)

### T108. `SPREAD-ELEMENT-EMPTY-TUPLE` (1; blocked 1, finished alone 0, lines 8; confidence high)

- **Root cause / port plan:** checkSpreadExpression on an empty tuple `[]` yields the iterated element type `never`; TSR array_spread_element_type has no answer for the zero-element tuple -> error.
- **tsgo:** internal/checker/checker.go checkSpreadExpression -> checkIteratedTypeOrElementType (getIteratedTypeOrElementType of [] = never)
- **TSR:** crates/tsr-checker/src/expressions.rs check_expression SpreadElement arm (~L999) / array_spread_element_type
- **Example:** `conformance/genericRestParameters2` [0:147] `...t4`. want `never`, got `error`
- **Example:** `conformance/genericRestParameters2` [0:154] `...t4`. want `never`, got `error`
- **Wave-1 evidence:** conformance/genericRestParameters2: `...t4` want never / got error; oracle `f(...t4)` same
- **Cases:** conformance/genericRestParameters2 (+CONDITIONAL-DEFERRAL-GATE)

### T109. `INTRA-EXPRESSION-INFERENCE-JSX-SPREAD` (1; blocked 1, finished alone 0, lines 8; confidence medium)

- **Root cause / port plan:** Intra-expression inference for an object literal in a JSX spread attribute (`<Foo {...{ a: (x) => 10, b: (arg) => ... }} />`): the literal is contextually typed by the generic props type; context-insensitive member `a` is recorded via addIntraExpressionInferenceSite so T=number before `b`'s arrow is contextually typed. TSR errors the spread object (b's arrow has no grounded contextual signature).
- **tsgo:** internal/checker/inference.go:1285 addIntraExpressionInferenceSite / :1302 inferFromIntraExpressionSites (JSX attrs: jsx.go:709 createJsxAttributesTypeFromAttributesProperty)
- **TSR:** crates/tsr-checker/src/jsx*.rs / contextual.rs JSX spread-attribute contextual typing + signatures.rs get_type_of_function_expression gate
- **Example:** `conformance/intraExpressionInferencesJsx` [0:117] `{  a: (x) => 10,  b: (arg) => { arg.toString(); },}`. want `{ a: (x: string) => number; b: (arg: number) => void; }`, got `error`
- **Example:** `conformance/intraExpressionInferencesJsx` [0:122] `b`. want `(arg: number) => void`, got `error`
- **Wave-1 evidence:** conformance/intraExpressionInferencesJsx: `{ a: (x) => 10, b: (arg) => {..} }` want { a: (x: string) => number; b: (arg: number) => void; } / got error (the attribute form `a={..} b={..}` is RIGHT)
- **Cases:** conformance/intraExpressionInferencesJsx (+CLONE-BINDING-NAME)

### T110. `BINDING-PATTERN-CONTEXT-LITERAL` (1; blocked 1, finished alone 0, lines 8; confidence medium)

- **Root cause / port plan:** Object literal initializer contextually typed by the implied type of a destructuring pattern: properties with defaults become optional and keep literal types (literal contextual type from the default).
- **tsgo:** internal/checker/checker.go:13144 checkObjectLiteral (contextualTypeHasPattern arm ~13169/13252) with getTypeFromBindingPattern 17904 as contextual type
- **TSR:** crates/tsr-checker/src/objects.rs:857 check_object_literal (no pattern-implied contextual type)
- **Example:** `conformance/literalTypes2` [0:299] `c2`. want `0 \| 1`, got `number`
- **Example:** `conformance/literalTypes2` [0:301] `c3`. want `"bar" \| "foo"`, got `string`
- **Wave-1 evidence:** literalTypes2: const {c1 = true, c2 = 0, c3 = 'foo'} = {c1:false,c2:1,c3:'bar'} want `{ c1?: false; c2?: 1; c3?: "bar"; }` got `{ c1?: boolean; ... }`
- **Cases:** conformance/literalTypes2 (+INFERENCE-SUPERTYPE-NAMED-UNION, ORIGIN-SLICE-GATE, REMOVE-SUBTYPES-UNDECIDABLE)

### T111. `CONTEXTUAL-DEFERRED-CONDITIONAL` (1; blocked 1, finished alone 0, lines 7; confidence low)

- **Root cause / port plan:** Object-literal argument contextually typed by a parameter that is a deferred conditional type (via its constraint/instantiated branches) so callback params get types.
- **tsgo:** internal/checker/checker.go getApparentTypeOfContextualType / instantiateContextualType
- **TSR:** crates/tsr-checker/src/contextual.rs
- **Example:** `compiler/complicatedIndexesOfIntersectionsAreInferencable` [0:8] `{    initialValues: {        foo: ""    },    validate: props => {        props.foo;    }}`. want `{ initialValues: { foo: string; }; validate: (props: { foo: string; }) => void; }`, got `error`
- **Example:** `compiler/complicatedIndexesOfIntersectionsAreInferencable` [0:13] `validate`. want `(props: { foo: string; }) => void`, got `error`
- **Wave-1 evidence:** complicatedIndexesOfIntersectionsAreInferencable `props` want { foo: string; } got any. Not minimised
- **Cases:** compiler/complicatedIndexesOfIntersectionsAreInferencable (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### T112. `TUPLE-OPTIONAL-ELEMENT-UNDEFINED` (1; blocked 1, finished alone 0, lines 7; confidence high)

- **Root cause / port plan:** Optional tuple element types include undefined (addOptionality) and print as `(T | undefined)?` under strictNullChecks.
- **tsgo:** internal/checker/checker.go:24205 getTypeFromOptionalTypeNode (addOptionality) + nodebuilder tuple printing
- **TSR:** crates/tsr-checker/src/tuples.rs (optional element type stored without undefined)
- **Example:** `compiler/destructuringAssignmentWithDefault` [0:43] `options`. want `[(string \| undefined)?, (number \| undefined)?] \| undefined`, got `[string?, number?] \| undefined`
- **Example:** `compiler/destructuringAssignmentWithDefault` [0:46] `options \|\| []`. want `[(string \| undefined)?, (number \| undefined)?]`, got `[string?, number?]`
- **Wave-1 evidence:** oracle: `declare const t: [string?, number?]` tsgo [(string \| undefined)?, (number \| undefined)?], TSR [string?, number?]
- **Cases:** compiler/destructuringAssignmentWithDefault (+REMOVE-SUBTYPES-UNDECIDABLE, UNION-PROP-OBJECT-LITERAL-MISSING)

### T113. `ARG-FINAL-CONTEXT-RECHECK` (1; blocked 1, finished alone 0, lines 7; confidence medium)

- **Root cause / port plan:** Argument expression types in a generic call are those of isSignatureApplicable's final checkExpressionWithContextualType against the INSTANTIATED signature's parameter type (e.g. `[...H<[string, boolean]>]` -> value contextual `boolean`); isLiteralOfContextualType treats `boolean` as a union of boolean literals so `true` stays literal. TSR records widened types from the inference pass / never re-derives the contextual type through the instantiated homomorphic mapped tuple.
- **tsgo:** internal/checker/checker.go:9256 isSignatureApplicable (checkExpressionWithContextualType) + :25522 isLiteralOfContextualType
- **TSR:** crates/tsr-checker/src/calls.rs choose_overload/argument recheck; contextual.rs contextual_type_for_argument (instantiated mapped tuple context)
- **Example:** `compiler/homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1` [0:22] `other`. want `{ label: string; options: [{ value: string; }, { value: true; }]; }`, got `{ label: string; options: [{ value: string; }, { value: boolean; }]; }`
- **Example:** `compiler/homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1` [0:23] `{        label: "second",        options: [            {                value: "bar",            }, `. want `{ label: string; options: [{ value: string; }, { value: true; }]; }`, got `{ label: string; options: [{ value: string; }, { value: boolean; }]; }`
- **Wave-1 evidence:** homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1: {value: true} want {value: true} got {value: boolean}; oracle probe f1<T extends readonly any[]>(fields:[...H<T>]) same; f3({value:true}) property-name line `value` also boolean vs true
- **Cases:** compiler/homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1

### T114. `CONTEXTUAL-PROPERTY-OWN-MEMBERS` (1; blocked 1, finished alone 0, lines 6; confidence high)

- **Root cause / port plan:** getTypeOfPropertyOfContextualTypeEx maps union constituents through getTypeOfConcretePropertyOfContextualType (declared/resolved members only, then index infos); a function-type constituent contributes nothing for a name like `apply`, so `{ apply: (compiler) => {} }` against `WPI | ((this: C, c: C) => void)` is typed from WPI alone. TSR includes the apparent Function.apply member, so the contextual signature collapses and the literal/arrow answer error
- **tsgo:** internal/checker/checker.go:30555 getTypeOfPropertyOfContextualTypeEx (getTypeOfConcretePropertyOfContextualType)
- **TSR:** crates/tsr-checker/src/contextual.rs union_contextual_property_type / contextual_type_for_object_literal_named_element (~L2143-2259)
- **Example:** `compiler/contextualTypeCaching` [0:42] `() => [    {      apply: (compiler) => {},    },  ]`. want `() => { apply: (compiler: MyCompiler) => void; }[]`, got `() => any`
- **Example:** `compiler/contextualTypeCaching` [0:43] `[    {      apply: (compiler) => {},    },  ]`. want `{ apply: (compiler: MyCompiler) => void; }[]`, got `error`
- **Wave-1 evidence:** oracle: `const x4: WPI \| (() => void) = { apply: (compiler) => {} }` tsgo compiler: MyCompiler, TSR error/any; same with property `foo` works in TSR
- **Cases:** compiler/contextualTypeCaching (+COMPARE-TYPES-ARMS)

### T115. `FLOW-LOOP-INCOMPLETE-TYPES` (1; blocked 1, finished alone 0, lines 6; confidence high)

- **Root cause / port plan:** getTypeAtFlowLoopLabel: while a loop label is being analysed for a reference, re-entry (an assignment RHS in the loop that itself references the variable through a narrowing) returns the union of the antecedent types computed so far (incomplete types, flowLoopStack), converging to e.g. string | number. TSR falls back to the declared type on that re-entry.
- **tsgo:** internal/checker/flow.go:1325 getTypeAtFlowLoopLabel
- **TSR:** crates/tsr-checker/src/flow.rs get_type_at_flow_loop_label (~L3018)
- **Example:** `conformance/typeGuardsAsAssertions` [0:43] `x`. want `string \| number`, got `string \| number \| boolean`
- **Example:** `conformance/typeGuardsAsAssertions` [0:49] `x`. want `string \| number`, got `string \| number \| boolean`
- **Wave-1 evidence:** typeGuardsAsAssertions foo1: in loop `x = typeof x === "string" ? x.slice() : "abc"` -> x want string \| number got string \| number \| boolean; p22.ts: without the self-reference (foo3) TSR gets string \| number
- **Cases:** conformance/typeGuardsAsAssertions (+FLOW-ASSIGNMENT-REDUCED-GENERIC, FLOW-NONNULL-DECLARED-FALLBACK)

### T116. `PRINT-ALIAS-DEFAULT-TYPEARGS` (1; blocked 1, finished alone 0, lines 5; confidence high)

- **Root cause / port plan:** A reference to a generic alias written without args gets its defaults filled into alias.typeArguments, so it prints `ReactType<any>`.
- **tsgo:** internal/checker/checker.go:23580 getTypeFromTypeAliasReference (getTypeArgumentsFromNode fills defaults)
- **TSR:** crates/tsr-checker/src/declared.rs qualified/alias reference minting (prints written text)
- **Example:** `compiler/callsOnComplexSignatures` [0:248] `component`. want `React.ReactType<any>`, got `React.ReactType`
- **Example:** `compiler/callsOnComplexSignatures` [0:250] `Comp`. want `React.ReactType<any>`, got `React.ReactType`
- **Wave-1 evidence:** callsOnComplexSignatures want React.ReactType<any> got React.ReactType; local `declare namespace R { type ReactType<P = any> = ... }` reproduces
- **Cases:** compiler/callsOnComplexSignatures (+QUALIFIED-NAME-LEFT-ALIAS-RESOLVE, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, SYNTHETIC-DEFAULT-IMPORT-TARGET)

### T117. `DISCRIMINATE-CONTEXTUAL-BY-MEMBERS` (1; blocked 1, finished alone 0, lines 4; confidence high)

- **Root cause / port plan:** discriminateContextualTypeByObjectMembers discriminates a union contextual type using EVERY discriminant-capable member: property initializers via getContextFreeTypeOfExpression (identifiers like `kind: kind` too) and shorthand members via their name. TSR's discriminate_union_root slice only accepts string/numeric/boolean literal initializers, so `{ kind, method(a) {} }` stays undiscriminated and `a` is any
- **tsgo:** internal/checker/checker.go:30755 discriminateContextualTypeByObjectMembers (ObjectLiteralDiscriminator.matches :30730, getContextFreeTypeOfExpression :7542)
- **TSR:** crates/tsr-checker/src/symbols.rs discriminate_union_root (~L3223, literal-only discriminators)
- **Example:** `compiler/contextuallyTypedByDiscriminableUnion` [0:37] `{    kind,    method(a) {        return +a;    }}`. want `{ kind: "a"; method(a: string): number; }`, got `{ kind: string; method(a: any): number; }`
- **Example:** `compiler/contextuallyTypedByDiscriminableUnion` [0:39] `method`. want `(a: string) => number`, got `(a: any) => number`
- **Wave-1 evidence:** compiler/contextuallyTypedByDiscriminableUnion: `invoke({ kind, method(a) {...} })` want a: string, got any; probe `{ kind: kind, m(a) {} }` also any while `{ kind: "a", ... }` works
- **Cases:** compiler/contextuallyTypedByDiscriminableUnion (+SHORTHAND-PROPERTY-CONTEXTUAL-LITERAL)

### T118. `ARRAY-LITERAL-TUPLE-CONTEXT` (1; blocked 1, finished alone 0, lines 4; confidence high)

- **Root cause / port plan:** checkArrayLiteral's inTupleContext: contextual type (apparent type of a type parameter = its constraint, e.g. `readonly unknown[] | []`) containing a tuple-like member makes `[]` a tuple `[]`; TSR yields never[].
- **tsgo:** internal/checker/checker.go:8021 checkArrayLiteral (L8029 inTupleContext someType(isTupleLikeType||generic homomorphic mapped))
- **TSR:** crates/tsr-checker/src/array_literals.rs (tuple-context decision with type-parameter contextual type)
- **Example:** `compiler/doYouNeedToChangeYourTargetLibraryES2015` [0:329] `testPromiseAll`. want `Promise<[]>`, got `Promise<never[]>`
- **Example:** `compiler/doYouNeedToChangeYourTargetLibraryES2015` [0:330] `Promise.all([])`. want `Promise<[]>`, got `Promise<never[]>`
- **Wave-1 evidence:** doYouNeedToChangeYourTargetLibraryES2015: `Promise.all([])` want Promise<[]> / got Promise<never[]>; oracle `pa<T extends readonly unknown[] \| []>(v:T); pa([])` tsgo [] / TSR never[] (Readonly<A> case already right)
- **Cases:** compiler/doYouNeedToChangeYourTargetLibraryES2015 (+CHOOSE-OVERLOAD-CONTEXT-SENSITIVE-ARG-RETENTION, CONDITIONAL-INLINE-NODE-INSTANTIATION, INFER-NO-CANDIDATE-GUARD, SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### T119. `BINDER-ALIAS-EXPORT-CONTEXT` (1; blocked 1, finished alone 0, lines 4; confidence high)

- **Root cause / port plan:** declareModuleMember handles Alias symbols BEFORE the ExportContext test: an import-equals without `export` goes to the container's locals even inside an ambient (ExportContext) module/namespace. TSR applies that only at SourceFile level, so `import X = ns` inside `declare module "m"`/`declare namespace` is bound into exports and `typeof X`/`X` fail to resolve.
- **tsgo:** internal/binder/binder.go:373 declareModuleMember (:376-381)
- **TSR:** crates/tsr-binder/src/binder.rs is_exported_from_container (L2906-2918: ImportEqualsDeclaration arm gated to SourceFile container)
- **Example:** `compiler/privacyGloImport` [0:115] `use_glo_M1_public_v2_public`. want `typeof use_glo_M1_public`, got `error`
- **Example:** `compiler/privacyGloImport` [0:116] `use_glo_M1_public`. want `typeof use_glo_M1_public`, got `error`
- **Wave-1 evidence:** probe d.ts: `declare module "other" { import xx = glo; export var b: typeof xx; let d = xx; }` → b/xx/d error in TSR, tsgo `typeof xx`; privacyGloImport [0:115/116/121/122]
- **Cases:** compiler/privacyGloImport (+AMBIENT-MODULE-BIND-VALUEMODULE, SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS)

### T120. `CONTEXTUAL-SIGNATURE-INTERSECTED` (1; blocked 1, finished alone 0, lines 4; confidence medium)

- **Root cause / port plan:** When the contextual type has several applicable call signatures (overloaded console.log), getContextualCallSignature combines them via getIntersectedSignatures/combineSignaturesOfIntersectionMembers (rest params merged → `(args_0?: any, ...args: any[])`, args: `[any?, ...any[]]`). TSR finds no contextual signature and refuses the arrow.
- **tsgo:** internal/checker/checker.go:10305 getContextualCallSignature; :10314 getIntersectedSignatures
- **TSR:** crates/tsr-checker/src/contextual.rs contextual_signature / signatures.rs get_type_of_function_expression
- **Example:** `compiler/signatureCombiningRestParameters2` [0:9] `console.log = (...args) => {  logs.push(...args);}`. want `(args_0?: any, ...args: any[]) => void`, got `error`
- **Example:** `compiler/signatureCombiningRestParameters2` [0:13] `(...args) => {  logs.push(...args);}`. want `(args_0?: any, ...args: any[]) => void`, got `error`
- **Wave-1 evidence:** signatureCombiningRestParameters2 `console.log = (...args) => {...}` want `(args_0?: any, ...args: any[]) => void` / got error; `args` want `[any?, ...any[]]` / got any[]
- **Cases:** compiler/signatureCombiningRestParameters2 (+DECLNAME-LIB-MERGED-SYMBOL)

### T121. `THIS-ARG-INSTANTIATE-MEMBER` (1; blocked 1, finished alone 0, lines 4; confidence medium)

- **Root cause / port plan:** Members of a class reference C<{}> are resolved with a mapper that also maps the polymorphic this type to the receiver; a member type `this extends C ? undefined : null` is instantiated and resolves.
- **tsgo:** internal/checker/checker.go:19106 resolveObjectTypeMembers (thisArgument mapper), :19095 resolveTypeReferenceMembers
- **TSR:** crates/tsr-checker/src/members.rs get_type_of_property_with_this_argument (this not substituted into conditional types)
- **Example:** `compiler/thisConditionalOnMethodReturnOfGenericInstance` [0:12] `y`. want `string \| undefined`, got `string \| this extends C ? undefined : null`
- **Example:** `compiler/thisConditionalOnMethodReturnOfGenericInstance` [0:13] `x.method()`. want `string \| undefined`, got `string \| this extends C ? undefined : null`
- **Wave-1 evidence:** thisConditionalOnMethodReturnOfGenericInstance 0:13 x.method(): want string \| undefined / got string \| this extends C ? undefined : null (probed m2 too)
- **Cases:** compiler/thisConditionalOnMethodReturnOfGenericInstance [type-operators] (+PRINT-TYPE-PRECEDENCE)

### T122. `LITERAL-CONTEXTUAL-INSTANTIABLE` (1; blocked 1, finished alone 0, lines 4; confidence medium)

- **Root cause / port plan:** isLiteralOfContextualType keeps a literal fresh when the contextual type is (or contains) an instantiable type whose constraint admits literals — here the property type of Narrow<TNarrow> (conditional | mapped over a type parameter).
- **tsgo:** internal/checker/checker.go:25522 isLiteralOfContextualType
- **TSR:** crates/tsr-checker/src/literals.rs / contextual.rs (literal-context check through conditional|mapped contextual types)
- **Example:** `compiler/typePredicateFreshLiteralWidening` [0:18] `{ value: "1" }`. want `{ value: "1"; }`, got `{ value: string; }`
- **Example:** `compiler/typePredicateFreshLiteralWidening` [0:19] `value`. want `"1"`, got `string`
- **Wave-1 evidence:** typePredicateFreshLiteralWidening 0:18 { value: "1" }: want { value: "1"; } / got { value: string; }; probed f<T>(x: Narrow<T>) -> {value: string}, plain homomorphic mapped ok
- **Cases:** compiler/typePredicateFreshLiteralWidening (+TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION)

### T123. `INFERENCE-REVERSE-MAPPED-MEMBER` (1; blocked 1, finished alone 0, lines 4; confidence medium)

- **Root cause / port plan:** Member types of a reverse-mapped inference (getTypeOfReverseMappedSymbol -> inferReverseMappedType) when the target is a nested/generic homomorphic mapped type (Boxified<Pick<T, K>>) or the mapped source is over an intersection (Pick<T & U, K>): upstream infers each member through the template (number/string). TSR produces the reverse-mapped object with `unknown` members.
- **tsgo:** internal/checker/inference.go:1066 inferReverseMappedType / :1145 getTypeOfReverseMappedSymbol (from inferToMappedType :948)
- **TSR:** crates/tsr-checker/src/inference.rs reverse_mapped_member_type(_worker) (~L3275-3450)
- **Example:** `conformance/isomorphicMappedTypeInference` [0:318] `x2`. want `{ foo: number; bar: string; }`, got `{ foo: unknown; bar: unknown; }`
- **Example:** `conformance/isomorphicMappedTypeInference` [0:319] `f22({ foo: { value: 42} , bar: { value: "hello" } })`. want `{ foo: number; bar: string; }`, got `{ foo: unknown; bar: unknown; }`
- **Wave-1 evidence:** conformance/isomorphicMappedTypeInference: f22({ foo: { value: 42 }, bar: { value: "hello" } }) want { foo: number; bar: string; } / got { foo: unknown; bar: unknown; }; f24 same with intersection
- **Cases:** conformance/isomorphicMappedTypeInference (+FORIN-VARIABLE-INDEX-TYPE, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, OBJLIT-PROPERTY-SYMBOL-TYPE)

### T124. `REVERSE-MAPPED-TUPLE` (1; blocked 1, finished alone 0, lines 4; confidence high)

- **Root cause / port plan:** Inference to a homomorphic mapped type from a tuple/array source builds a reverse-mapped tuple element-wise (incl. rest elements, optional->required for `?` modifier).
- **tsgo:** internal/checker/inference.go:1014 createReverseMappedType (tuple arm 1029)
- **TSR:** crates/tsr-checker/src/inference.rs reverse-mapped inference (declines rest/variadic tuples)
- **Example:** `conformance/mappedTypesArraysTuples` [0:34] `y10`. want `[number, string, ...boolean[]]`, got `error`
- **Example:** `conformance/mappedTypesArraysTuples` [0:35] `unboxify(x10)`. want `[number, string, ...boolean[]]`, got `error`
- **Wave-1 evidence:** mappedTypesArraysTuples: unboxify(x10: [Box<number>, Box<string>, ...Box<boolean>[]]) want [number, string, ...boolean[]] got error; fixed tuple works
- **Cases:** conformance/mappedTypesArraysTuples (+MAPPED-TUPLE-TEMPLATE-INSTANTIATION, TUPLE-OPTIONAL-ELEMENT-OPTIONALITY)

### T125. `RETURN-LITERAL-OWN-SIG-CONTEXT` (1; blocked 1, finished alone 0, lines 3; confidence medium)

- **Root cause / port plan:** getReturnTypeFromBody keeps a unit return type when the contextual signature IS the function's own signature (contextual type is a type parameter inferred from the function itself, e.g. ext<A>(a: A) with {f: () => 's'}).
- **tsgo:** internal/checker/checker.go:20209 getReturnTypeFromBody (contextualSignature == getSignatureFromDeclaration(fn) arm)
- **TSR:** crates/tsr-checker/src/signatures.rs:2497 return_type_from_body (widens)
- **Example:** `compiler/amdLikeInputDeclarationEmit` [2:12] `{        f: function() {            return "something";        }    }`. want `{ f: () => "something"; }`, got `{ f: () => string; }`
- **Example:** `compiler/amdLikeInputDeclarationEmit` [2:13] `f`. want `() => "something"`, got `() => string`
- **Wave-1 evidence:** amdLikeInputDeclarationEmit want `f : () => "something"` got () => string; minimised `ext<A>(a:A); ext(() => "s")` -> tsgo () => "s"
- **Cases:** compiler/amdLikeInputDeclarationEmit (+IMPORT-TYPE-NODE-TYPEOF, JS-LITERAL-PROPERTY-ANY)

### T126. `CONDITIONAL-DISTRIBUTIVE-CONSTRAINT` (1; blocked 1, finished alone 0, lines 3; confidence medium)

- **Root cause / port plan:** Constraint of a distributive conditional (Extract<M[K], ArrayLike<any>>) for narrowable references distributes over the check type's constraint.
- **tsgo:** internal/checker/checker.go:17284 getConstraintOfDistributiveConditionalType (via 31491 getNarrowableTypeForReference)
- **TSR:** crates/tsr-checker/src/constraints.rs default_constraint_of_conditional_type
- **Example:** `compiler/deeplyNestedConstraints` [0:6] `array.length`. want `number`, got `error`
- **Example:** `compiler/deeplyNestedConstraints` [0:7] `array`. want `string \| number[]`, got `string \| number \| boolean \| number[]`
- **Wave-1 evidence:** deeplyNestedConstraints: array want string \| number[] / got string \| number \| boolean \| number[]
- **Cases:** compiler/deeplyNestedConstraints (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS)

### T127. `LITERAL-OF-CONTEXTUAL-TYPEVAR` (1; blocked 1, finished alone 0, lines 3; confidence medium)

- **Root cause / port plan:** Object-literal property literals under a GENERIC contextual type: checkExpressionForMutableLocation -> getWidenedLiteralLikeTypeForContextualType -> isLiteralOfContextualType with contextual type T["y"] (type-variable arm: constraint `Types` contains string literals) keeps `"string"` fresh. TSR widens the property to `string` in the property line and in the argument type used for inference, so the inferred T fails its constraint and inference falls back to the constraint.
- **tsgo:** internal/checker/checker.go:25522 isLiteralOfContextualType (TypeVariable arm) via :25515 getWidenedLiteralLikeTypeForContextualType, :30551 getTypeOfPropertyOfContextualType; consumed by :9390 inferTypeArguments
- **TSR:** crates/tsr-checker/src/signatures.rs is_literal_of_contextual_type (L3758) / object-literal property widening on the inference road
- **Example:** `conformance/accessorsOverrideProperty8` [0:10] `Base`. want `{ new (): Base & Properties<{ readonly x: "boolean"; y: "string"; }>; prototype: Base & Properties<{ readonly x: "boolean"; y: "string"; }>; }`, got `{ new (): Base & Properties<{ [key: string]: Types; }>; prototype: Base & Properties<{ [key: string]: Types; }>; }`
- **Example:** `conformance/accessorsOverrideProperty8` [0:11] `classWithProperties({    get x() { return 'boolean' as const },    y: 'string',}, class Base {})`. want `{ new (): Base & Properties<{ readonly x: "boolean"; y: "string"; }>; prototype: Base & Properties<{ readonly x: "boolean"; y: "string"; }>; }`, got `{ new (): Base & Properties<{ [key: string]: Types; }>; prototype: Base & Properties<{ [key: string]: Types; }>; }`
- **Wave-1 evidence:** minimised: `declare function h<T extends { y: Types }>(p: T): T; h({ y: 'string' })` TSR `>y : string`, call `{ y: Types; }` (constraint); with `'string' as const` inference works. accessorsOverrideProperty8: want Properties<{ readonly x: "boolean"; y: "string"; }> got Properties<{ [key: string]: Types; }>
- **Cases:** conformance/accessorsOverrideProperty8 (+CLASS-GET-BASE-TYPES)

### T128. `FLOW-IN-NONLITERAL-KEY` (1; blocked 1, finished alone 0, lines 3; confidence high)

- **Root cause / port plan:** `k in x` narrowing where the left operand is not a written string literal but an expression whose type is usable as a property name (e.g. const a = 'a'): upstream uses getTypeOfExpression(expr.Left) + isTypeUsableAsPropertyName; TSR's in-narrowing keeps a written-literal boundary so the identifier key does not narrow.
- **tsgo:** internal/checker/flow.go:530 narrowTypeByBinaryExpression InKeyword arm (getTypeOfExpression(expr.Left) -> narrowTypeByInKeyword)
- **TSR:** crates/tsr-checker/src/flow.rs narrow_type InKeyword arm (~L5080, written-literal boundary) -> narrow_type_by_in_keyword
- **Example:** `conformance/controlFlowInOperator` [0:27] `c`. want `A`, got `A \| B`
- **Example:** `conformance/controlFlowInOperator` [0:28] `c[a]`. want `number`, got `error`
- **Wave-1 evidence:** controlFlowInOperator: if (a in c) { c } with const a = 'a' want A / got A \| B; `'a' in c` narrows fine in probe
- **Cases:** conformance/controlFlowInOperator (+FLOW-IN-RECORD-INTERSECT)

### T129. `DESTRUCTURE-FLOW-DEPENDENT` (1; blocked 1, finished alone 0, lines 3; confidence medium)

- **Root cause / port plan:** Destructured variables from a discriminated union narrow each other (dependent destructuring). Upstream's getFlowTypeOfDestructuring + discriminant narrowing of the sibling narrows p1 to number. TSR answers the declared slice; the destructure.rs module doc names this as unported.
- **tsgo:** internal/checker/checker.go getFlowTypeOfDestructuring (called from :17707 getBindingElementTypeFromParentType); flow.go getCandidateDiscriminantPropertyAccess
- **TSR:** crates/tsr-checker/src/destructure.rs (getFlowTypeOfDestructuring unported, module doc 'The named risk')
- **Example:** `conformance/dependentDestructuredVariablesFromNestedPatterns` [0:6] `p1`. want `number`, got `number \| undefined`
- **Example:** `conformance/dependentDestructuredVariablesFromNestedPatterns` [0:11] `p1`. want `number`, got `number \| undefined`
- **Wave-1 evidence:** dependentDestructuredVariablesFromNestedPatterns: test1 `const [[p1, p1Error]] = arg; if (p1Error) return; p1` want number / got number \| undefined
- **Cases:** conformance/dependentDestructuredVariablesFromNestedPatterns (+APPARENT-MAPPED-ARRAY-CONSTRAINT, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS, UNATTRIBUTED)

### T130. `ARG-CONTEXT-RESOLVED-SIG` (1; blocked 1, finished alone 0, lines 2; confidence medium)

- **Root cause / port plan:** After a generic call resolves, re-checking an argument uses the parameter type of the RESOLVED (instantiated) signature as contextual type, so literals inside tuple-shaped args stay literal (isLiteralOfContextualType vs instantiated V=boolean).
- **tsgo:** internal/checker/checker.go:29772 getContextualTypeForArgumentAtIndex (reads getResolvedSignature)
- **TSR:** crates/tsr-checker/src/contextual.rs:2348 contextual_type_for_argument (memo/uninstantiated roads)
- **Example:** `compiler/acceptSymbolAsWeakType` [0:27] `[[s, false]]`. want `[symbol, false][]`, got `[symbol, boolean][]`
- **Example:** `compiler/acceptSymbolAsWeakType` [0:28] `[s, false]`. want `[symbol, false]`, got `[symbol, boolean]`
- **Wave-1 evidence:** acceptSymbolAsWeakType: want `[s, false] : [symbol, false]` got `[symbol, boolean]`; minimised `h<V>(e: readonly [V]); h([false])` -> tsgo [false], TSR [boolean]; h<boolean>([false]) agrees
- **Cases:** compiler/acceptSymbolAsWeakType (+INFER-NO-CANDIDATE-GUARD)

### T131. `NARROW-INSTANCEOF-HASINSTANCE` (1; blocked 1, finished alone 0, lines 2; confidence low)

- **Root cause / port plan:** narrowTypeByInstanceof uses a right operand's [Symbol.hasInstance] method type predicate (`value is ...`) to narrow; TSR narrows by construct-signature instance type only
- **tsgo:** internal/checker/checker.go narrowTypeByInstanceof (getSymbolHasInstanceMethodOfObjectType -> type predicate)
- **TSR:** crates/tsr-checker/src/flow.rs instanceof narrowing
- **Example:** `compiler/controlFlowInstanceofWithSymbolHasInstance` [0:141] `x`. want `X & Y`, got `Y`
- **Example:** `compiler/controlFlowInstanceofWithSymbolHasInstance` [0:143] `x`. want `X`, got `X \| Y`
- **Wave-1 evidence:** compiler/controlFlowInstanceofWithSymbolHasInstance: x want `X & Y` got `Y`, want `X` got `X \| Y` (unminimised)
- **Cases:** compiler/controlFlowInstanceofWithSymbolHasInstance (+MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT)

### T132. `FLOW-IN-RECORD-INTERSECT` (1; blocked 1, finished alone 0, lines 2; confidence high)

- **Root cause / port plan:** `"p" in x` where no constituent of x declares p: upstream narrowTypeByInKeyword (assumeTrue) intersects with Record<"p", unknown> via the global Record alias; TSR returns t unchanged (Record alias instantiation unported).
- **tsgo:** internal/checker/flow.go:1001 narrowTypeByInKeyword (Record intersection arm :1014)
- **TSR:** crates/tsr-checker/src/flow.rs narrow_type_by_in_keyword (`if !known { return t; }`)
- **Example:** `conformance/controlFlowInOperator` [0:23] `c`. want `(A \| B) & Record<"d", unknown>`, got `A \| B`
- **Example:** `conformance/controlFlowInOperator` [0:34] `c`. want `(A \| B) & Record<"d", unknown>`, got `A \| B`
- **Wave-1 evidence:** controlFlowInOperator: if ('d' in c) c want (A \| B) & Record<"d", unknown> / got A \| B
- **Cases:** conformance/controlFlowInOperator (+FLOW-IN-NONLITERAL-KEY)

### T133. `INFERENCE-REVERSE-MAPPED-INTERSECTION` (1; blocked 1, finished alone 0, lines 2; confidence high)

- **Root cause / port plan:** Inference to a homomorphic mapped type `{[K in keyof T]: T[K]}` from an INTERSECTION source ({a}&{b}): inferToMappedType -> inferTypeForHomomorphicMappedType -> createReverseMappedType over the intersection's resolved properties. TSR reverse mapping handles plain object sources only; intersection source -> call error.
- **tsgo:** internal/checker/inference.go:948 inferToMappedType / :1004 inferTypeForHomomorphicMappedType / :1014 createReverseMappedType
- **TSR:** crates/tsr-checker/src/inference.rs reverse_mapped_member_plan / reverse_mapped_member_type (~L3275-3570)
- **Example:** `conformance/intersectionTypeInference2` [0:20] `f2(obj, 'a')`. want `string`, got `error`
- **Example:** `conformance/intersectionTypeInference2` [0:24] `f2(obj, 'b')`. want `string`, got `error`
- **Wave-1 evidence:** conformance/intersectionTypeInference2: f2(obj, 'a') want string / got error; probe f3(o2) with plain object works, f3(obj) intersection errors
- **Cases:** conformance/intersectionTypeInference2 (+TYPE-LITERAL-PROPERTY-ANNOTATION-REUSE)

### T134. `TUPLE-OPTIONAL-ELEMENT-OPTIONALITY` (1; blocked 1, finished alone 0, lines 2; confidence high)

- **Root cause / port plan:** Optional tuple element types get addOptionality (`string?` stores string|undefined and prints `(string | undefined)?`).
- **tsgo:** internal/checker/checker.go:24205 getTypeFromOptionalTypeNode (also 24164 named members)
- **TSR:** crates/tsr-checker/src/declared.rs tuple type-node arm (TupleTypeNode / OptionalType element)
- **Example:** `conformance/mappedTypesArraysTuples` [0:52] `x20`. want `[number \| undefined, (string \| undefined)?, ...boolean[]]`, got `[number \| undefined, string?, ...boolean[]]`
- **Example:** `conformance/mappedTypesArraysTuples` [0:56] `x20`. want `[number \| undefined, (string \| undefined)?, ...boolean[]]`, got `[number \| undefined, string?, ...boolean[]]`
- **Wave-1 evidence:** mappedTypesArraysTuples: declare let x20: [number\|undefined, string?, ...boolean[]] want `[..., (string \| undefined)?, ...]` got `string?`; oracle confirms on 1-line probe
- **Cases:** conformance/mappedTypesArraysTuples (+MAPPED-TUPLE-TEMPLATE-INSTANTIATION, REVERSE-MAPPED-TUPLE)

### T135. `FLOW-ASSIGNMENT-REDUCED-GENERIC` (1; blocked 1, finished alone 0, lines 2; confidence medium)

- **Root cause / port plan:** getAssignmentReducedType filters a declared union by typeMaybeAssignableTo(assigned, constituent); for generic interface instantiations (Some<r> vs None, {some: r} vs Some<r>) upstream decides structurally. TSR's reference_assignable_decidable/type_maybe_assignable_to cannot decide for a generic alias union and keeps the declared type.
- **tsgo:** internal/checker/flow.go:2399 getAssignmentReducedType (from flow.go:220 getTypeAtFlowAssignment)
- **TSR:** crates/tsr-checker/src/flow.rs get_assignment_reduced_type (~L2496)
- **Example:** `conformance/typeGuardsAsAssertions` [0:21] `result`. want `None`, got `Optional<r>`
- **Example:** `conformance/typeGuardsAsAssertions` [0:37] `result`. want `Some<r>`, got `Optional<r>`
- **Wave-1 evidence:** typeGuardsAsAssertions: `let result: Optional<r> = none; result` want None got Optional<r>; p21.ts: with concrete S<number>\|N it narrows to N, with generic O<r> stays O<r>
- **Cases:** conformance/typeGuardsAsAssertions (+FLOW-LOOP-INCOMPLETE-TYPES, FLOW-NONNULL-DECLARED-FALLBACK)

### T136. `OBJLIT-PROPERTY-NAME-CONTEXTUAL-LITERAL` (1; blocked 1, finished alone 0, lines 2; confidence medium)

- **Root cause / port plan:** The declaration-name line of an object-literal PropertyAssignment is getTypeOfSymbol -> checkPropertyAssignment -> checkExpressionForMutableLocation, which keeps a literal when the contextual type admits it (isLiteralOfContextualType; contextual type is a type parameter / its instantiation). TSR widens the member symbol's type (the literal's own printed type keeps `z: true`).
- **tsgo:** internal/checker/checker.go:16611 getTypeOfVariableOrParameterOrPropertyWorker (PropertyAssignment) -> checker.go:13673 checkPropertyAssignment -> checker.go:13878 checkExpressionForMutableLocation -> checker.go:25515 getWidenedLiteralLikeTypeForContextualType
- **TSR:** crates/tsr-checker/src/objects.rs member symbol type recorded via symbol_types.entry().or_insert (widened, context-free)
- **Example:** `conformance/typeParameterAsTypeParameterConstraintTransitively` [0:31] `z`. want `true`, got `boolean`
- **Example:** `conformance/typeParameterAsTypeParameterConstraintTransitively` [0:47] `hm`. want `true`, got `boolean`
- **Wave-1 evidence:** typeParameterAsTypeParameterConstraintTransitively: foo(..., { x: 2, y: '', z: true }) `z` want true got boolean; p25.ts f({z:true},{z:false}) literal prints z: true but property line z: boolean
- **Cases:** conformance/typeParameterAsTypeParameterConstraintTransitively (+GENERIC-ARG-NIL-CONTEXTUAL-SIGNATURE)

### T137. `AWAITED-THENABLE-EVAL` (1; blocked 1, finished alone 0, lines 1; confidence low)

- **Root cause / port plan:** Awaited<Promise3<...>> over a jQuery-style 12-type-param overloaded `then` evaluates to string; TSR errors. Single-overload reductions with few type params agree, so the trigger is in the full overload/type-param set.
- **tsgo:** internal/checker/checker.go:24300 getConditionalType / inference.go:838 inferFromSignatures
- **TSR:** crates/tsr-checker/src/declared.rs evaluate_conditional_inference (7252)
- **Example:** `compiler/awaitedTypeJQuery` [0:64] `T`. want `string`, got `error`
- **Wave-1 evidence:** awaitedTypeJQuery `T : string` got error; each of overloads 4 and 6 alone reproduce
- **Cases:** compiler/awaitedTypeJQuery (+SIGNATURE-DECLARATION-ANNOTATION-REUSE)

### T138. `INFERENCE-GENERIC-SOURCE-SIG` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** Inferring from a GENERIC source signature (e.g. Boolean `<T>(v?: T) => boolean`) to a type-predicate overload `<S extends T>(p: (v) => v is S)` must use getBaseSignature and let overload resolution proceed to the next candidate.
- **tsgo:** internal/checker/inference.go:838 inferFromSignatures; checker.go:19438 getBaseSignature
- **TSR:** crates/tsr-checker/src/inference.rs infer_from_types_with_priority (signature arm)
- **Example:** `compiler/booleanFilterAnyArray` [0:20] `realanys.filter(Boolean)`. want `any[]`, got `error`
- **Wave-1 evidence:** booleanFilterAnyArray realanys.filter(Boolean) want any[] got error; minimised overload ov<S>(p: v is S)/ov(p: unknown) with generic bp fails, non-generic nb works
- **Cases:** compiler/booleanFilterAnyArray (+CONDITIONAL-INLINE-NODE-INSTANTIATION)

### T139. `SHORTHAND-PROPERTY-CONTEXTUAL-LITERAL` (1; blocked 1, finished alone 0, lines 1; confidence high)

- **Root cause / port plan:** checkShorthandPropertyAssignment -> checkExpressionForMutableLocation(name) keeps the identifier's literal type when the contextual type contains that literal (getWidenedLiteralLikeTypeForContextualType); TSR widens a shorthand member (`{ k }` with const k = "a" against `"a" | "b"` prints `k: string`), while `{ k: k }` keeps "a"
- **tsgo:** internal/checker/checker.go:13229 checkShorthandPropertyAssignment -> :13878 checkExpressionForMutableLocation
- **TSR:** crates/tsr-checker/src/objects.rs / expressions.rs shorthand property member typing (widens unconditionally)
- **Example:** `compiler/contextuallyTypedByDiscriminableUnion` [0:38] `kind`. want `"a"`, got `string`
- **Wave-1 evidence:** compiler/contextuallyTypedByDiscriminableUnion: shorthand `kind` want "a" got string; probe `const o2: { k: "a" \| "b" } = { k }` -> `k : string`
- **Cases:** compiler/contextuallyTypedByDiscriminableUnion (+DISCRIMINATE-CONTEXTUAL-BY-MEMBERS)

### T140. `RELATER-NONPRIMITIVE-INDEX-SIGNATURE` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** `object` source vs target with string index signature is decidably not related (no implicit index signature), so overload resolution falls through.
- **tsgo:** internal/checker/relater.go:4603 typeRelatedToIndexInfo / 4578 indexSignaturesRelatedTo
- **TSR:** crates/tsr-checker/src/relater.rs (NON_PRIMITIVE source arms ~L1725; undecided -> error)
- **Example:** `compiler/declarationEmitMappedTypePreservesTypeParameterConstraint` [0:27] `Object.entries(o)`. want `[string, any][]`, got `error`
- **Wave-1 evidence:** minimal: overload m(o:{[s:string]:unknown}) / m(o:{}) with object arg -> TSR error, tsgo picks 2nd; Object.entries(o) want [string, any][]
- **Cases:** compiler/declarationEmitMappedTypePreservesTypeParameterConstraint [type-operators] (+DEFERRED-TYPE-REFERENCE-ALIAS, INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION)

### T141. `NARROWABLE-REF-INFERENTIAL-GATE` (1; blocked 1, finished alone 0, lines 1; confidence high)

- **Root cause / port plan:** getNarrowableTypeForReference skips constraint substitution only under CheckModeInferential; the final (non-inferential) argument check of a generic call still substitutes (k: K extends keyof S passed to `prop: PropertyKey` prints string | number | symbol). TSR returns the unsubstituted type whenever any inference context is active.
- **tsgo:** internal/checker/checker.go:31491 getNarrowableTypeForReference (checkMode&CheckModeInferential gate)
- **TSR:** crates/tsr-checker/src/constraints.rs narrowable_type_for_reference (`if !self.active_inference_contexts.is_empty() { return ty }`)
- **Example:** `compiler/indexedAccessAndNullableNarrowing` [0:45] `key`. want `string \| number \| symbol`, got `K`
- **Wave-1 evidence:** indexedAccessAndNullableNarrowing: hasOwnProperty(props, key) key want string \| number \| symbol got K; probe pk(k) right but generic hp(s, k) keeps K
- **Cases:** compiler/indexedAccessAndNullableNarrowing (+TYPE-ALIAS-INSTANTIATION-NEW-ALIAS)

### T142. `INDEXED-ACCESS-NEVER-INDEX` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** Indexed access with a never index (`object[keyof object]`) is never.
- **tsgo:** internal/checker/checker.go:27001 getPropertyTypeForIndexType (via 26935 getIndexedAccessTypeOrUndefined)
- **TSR:** crates/tsr-checker/src/indexed access evaluation
- **Example:** `compiler/jsFileImportPreservedWhenUsed` [2:17] `object => ({ ...object, [INDEX_FIELD]: index++ })`. want `(object: never) => any`, got `error`
- **Wave-1 evidence:** probe jp4: `declare var y: object[keyof object]` TSR error; callback `(v: T[keyof T])` with T=object errors
- **Cases:** compiler/jsFileImportPreservedWhenUsed (+MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS, TYPE-ALIAS-ACCESSIBILITY-GATE)

### T143. `JSX-GENERIC-COMPONENT-INFERENCE` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** JSX opening element on a generic class component infers type arguments (inferJsxTypeArguments); attributes then get literal contextual types.
- **tsgo:** internal/checker/jsx.go:197 inferJsxTypeArguments / 544 resolveJsxOpeningLikeElement
- **TSR:** crates/tsr-checker/src/jsx resolution for generic components
- **Example:** `compiler/jsxHasLiteralType` [0:9] `x`. want `"a"`, got `string`
- **Wave-1 evidence:** jsxHasLiteralType: `<MyComponent x="a"/>` want "a" got string; non-generic NC works
- **Cases:** compiler/jsxHasLiteralType (+CLASS-GET-BASE-TYPES)

### T144. `JSX-ATTR-CONTEXTUAL-LITERAL` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** checkJsxAttribute uses checkExpressionForMutableLocation; for a class component whose props are `Readonly<P & {...}>` with generic P, the attribute's contextual type is a generic indexed access so isLiteralOfContextualType keeps the literal (`name : "Matt"`). TSR widens to string.
- **tsgo:** internal/checker/jsx.go:871 checkJsxAttribute; checker.go:25522 isLiteralOfContextualType
- **TSR:** crates/tsr-checker/src/jsx_intrinsic.rs jsx_attribute_context (L723) / jsx_attributes_context (L92)
- **Example:** `compiler/reactReadonlyHOCAssignabilityReal` [0:17] `name`. want `"Matt"`, got `string`
- **Wave-1 evidence:** reactReadonlyHOCAssignabilityReal `name` want `"Matt"` / got string; probe r3.tsx: generic P → tsgo `"Matt"`, non-generic ComponentClass<{name:string}> → string in both
- **Cases:** compiler/reactReadonlyHOCAssignabilityReal (+CLASS-GET-BASE-TYPES, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER)

### T145. `GENERIC-REDUCIBLE-INDEXED-ACCESS` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** An indexed access whose object is a reducible intersection with a generic discriminant (`(Payload & { dataType: K })["data"]`) is a generic object type (ObjectFlagsIsGenericObjectType via isGenericReducibleType) and stays deferred; TSR reduces it eagerly to the union of data types.
- **tsgo:** internal/checker/checker.go:24880 getGenericObjectFlags; :24932 isGenericReducibleType; :24937 isReducibleIntersection; :26923 getIndexedAccessType
- **TSR:** crates/tsr-checker/src (indexed access resolution; get_indexed_access_type)
- **Example:** `compiler/reducibleIndexedAccessTypes` [0:19] `data`. want `(Payload & { dataType: K; })["data"]`, got `string \| number \| { x: number; y: number; }`
- **Wave-1 evidence:** reducibleIndexedAccessTypes `data` want `(Payload & { dataType: K; })["data"]` / got `string \| number \| { x: number; y: number; }`
- **Cases:** compiler/reducibleIndexedAccessTypes (+MAPPED-MEMBER-KEYS-ENUM-AND-ANY)

### T146. `DECLNAME-LIB-MERGED-SYMBOL` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** The declaration-name line of a member declared in a user interface that merges with a lib interface (`interface Console { log(...) }`) types the MERGED symbol (getSymbolOfDeclaration → getMergedSymbol), i.e. all overloads incl. lib ones. TSR types only the local declaration (property access through the merged type is already right).
- **tsgo:** internal/checker/checker.go:31991 getTypeOfNode IsDeclaration arm → :14390 getSymbolOfDeclaration → :14355 getMergedSymbol
- **TSR:** crates/tsr-conformance/src/types_producer.rs declaration-name arm / crates/tsr-checker/src/symbols.rs get_type_of_symbol (lib-merged symbol)
- **Example:** `compiler/signatureCombiningRestParameters2` [0:0] `log`. want `{ (...data: any[]): void; (message?: any, ...optionalParams: any[]): void; }`, got `(message?: any, ...optionalParams: any[]) => void`
- **Wave-1 evidence:** signatureCombiningRestParameters2 `log` want `{ (...data: any[]): void; (message?: any, ...optionalParams: any[]): void; }` / got single signature; probe z.ts: console.log access shows both
- **Cases:** compiler/signatureCombiningRestParameters2 (+CONTEXTUAL-SIGNATURE-INTERSECTED)

### T147. `PRINT-TYPE-PRECEDENCE` (1; blocked 1, finished alone 0, lines 1; confidence high)

- **Root cause / port plan:** Printer parenthesises union constituents by type precedence (conditional/function types inside a union get parens).
- **tsgo:** internal/printer/printer.go:2038 emitUnionTypeConstituent (emitTypeNode with TypePrecedenceTypeOperator)
- **TSR:** crates/tsr-checker/src/printing.rs union text (no parens for conditional constituents)
- **Example:** `compiler/thisConditionalOnMethodReturnOfGenericInstance` [0:4] `method`. want `() => string \| (this extends C ? undefined : null)`, got `() => string \| this extends C ? undefined : null`
- **Wave-1 evidence:** thisConditionalOnMethodReturnOfGenericInstance 0:4: want () => string \| (this extends C ? undefined : null) / got without parens; probed `string \| (T extends string ? 1 : 2)`
- **Cases:** compiler/thisConditionalOnMethodReturnOfGenericInstance [type-operators] (+THIS-ARG-INSTANTIATE-MEMBER)

### T148. `INDEXED-ACCESS-GENERIC-DEFER` (1; blocked 1, finished alone 0, lines 1; confidence high)

- **Root cause / port plan:** Element access whose index type is generic (T, D | (T & number)) yields a deferred indexed access type `(typeof D)[T]` (isGenericIndexType -> deferred) even when the object is an enum object with a numeric reverse-map index signature; TSR answers the index-signature value `string` for enum objects.
- **tsgo:** internal/checker/checker.go:26935 getIndexedAccessTypeOrUndefined (isGenericIndexType checker.go:24876)
- **TSR:** crates/tsr-checker/src/indexed.rs check_element_access_type / deferred_indexed_access (enum-object path resolves before deferral)
- **Example:** `conformance/generatorYieldContextualType` [0:32] `Directive[value]`. want `(typeof Directive)[Directive \| (T & number)]`, got `string`
- **Wave-1 evidence:** conformance/generatorYieldContextualType: `Directive[value]` want (typeof Directive)[Directive \| (T & number)] / got string; oracle `g<T extends number>(v:T){D[v]}` tsgo (typeof D)[T]
- **Cases:** conformance/generatorYieldContextualType (+APPEND-LOCAL-TYPE-PARAMETERS, CONDITIONAL-BRANCH-NAMED-UNION, CONTEXTUAL-RETURN-GENERATOR-FILTER, GENERATOR-ANNOTATION-ITERATION-TYPES, ORIGIN-ENTRY-ORDER, REMOVE-SUBTYPES-UNDECIDABLE, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE)

### T149. `OBJLIT-PROPERTY-SYMBOL-TYPE` (1; blocked 1, finished alone 0, lines 1; confidence high)

- **Root cause / port plan:** The property-assignment NAME line of an object literal reads getTypeOfSymbol(prop), which for an object-literal member is the type checkObjectLiteral recorded during the (contextual) check - literal preserved when the contextual type is a type parameter (`g<T>(v: T); g({ c: false })` -> `c : false`). TSR recomputes the member type from the declaration without the call-argument contextual type and widens it (`boolean`); the literal expression line itself is right.
- **tsgo:** internal/checker/checker.go:13144 checkObjectLiteral (member symbol links.resolvedType) + getWidenedLiteralLikeTypeForContextualType
- **TSR:** crates/tsr-checker/src/objects.rs check_object_literal symbol_types cache (§892/§890 exclusion of call arguments) / symbols.rs property-assignment type
- **Example:** `conformance/isomorphicMappedTypeInference` [0:122] `c`. want `false`, got `boolean`
- **Wave-1 evidence:** conformance/isomorphicMappedTypeInference: `assignBoxified(b, { c: false })` `c` want false / got boolean; oracle `g({ c: false })` same
- **Cases:** conformance/isomorphicMappedTypeInference (+FORIN-VARIABLE-INDEX-TYPE, INFERENCE-REVERSE-MAPPED-MEMBER, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT)

### T150. `MODULE-INSTANCE-STATE-EXPORT-ALIAS` (1; blocked 1, finished alone 0, lines 1; confidence high)

- **Root cause / port plan:** getModuleInstanceState for an ExportDeclaration/export specifier must follow getModuleInstanceStateForAliasTarget (type-only target => NonInstantiated). Drives writer's value-meaning skip of namespace names and typeof-vs-any of namespace references.
- **tsgo:** internal/ast/utilities.go:2352 getModuleInstanceStateWorker / utilities.go:2406 getModuleInstanceStateForAliasTarget
- **TSR:** crates/tsr-ast/src/lib.rs module_instance_state_at (ExportDeclaration falls to `_ => Instantiated`)
- **Example:** `conformance/reExportAliasMakesInstantiated` [0:12] `test1`. want `any`, got `string`
- **Wave-1 evidence:** reExportAliasMakesInstantiated: `declare namespace mod1 { type test1=string; export {test1} }` TSR prints `>mod1 : typeof mod1`, tsgo omits it (non-instantiated) -> 7 unaligned + shifted [0:12]
- **Cases:** conformance/reExportAliasMakesInstantiated

### T151. `INFERENCE-PRIMITIVE-CONSTRAINT-REGULAR` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** getCovariantInference: with a primitive constraint, candidates are mapped through getRegularTypeOfLiteralType (which maps union constituents too), so the inferred union of literals is non-fresh and a `let` initializer keeps it. TSR regularizes a single literal candidate but not a union candidate (from a conditional expression) -> getWidenedLiteralType widens to string.
- **tsgo:** internal/checker/inference.go:1434 getCovariantInference (primitiveConstraint -> getRegularTypeOfLiteralType)
- **TSR:** crates/tsr-checker/src/inference.rs inferred-type candidate handling
- **Example:** `conformance/templateLiteralTypes2` [0:143] `y2`. want `"a" \| `foo${string}``, got `string`
- **Wave-1 evidence:** templateLiteralTypes2: `let y2 = nonWidening(cond ? 'a' : `foo${s}`)` want "a" \| `foo${string}` got string; p14.ts: nw("a") ok, nw(cond?"a":"b") -> y2: string
- **Cases:** conformance/templateLiteralTypes2 (+CONDITIONAL-INLINE-NODE-INSTANTIATION)

### T152. `TEMPLATE-SPAN-CONSTRAINT-ASSIGNABILITY` (1; blocked 1, finished alone 0, lines 1; confidence medium)

- **Root cause / port plan:** checkTemplateExpression keeps each span's type if isTypeAssignableTo(t, templateConstraintType) else string. For a type parameter constrained by an alias instantiation (`Keyof<R>` = `keyof T & string`), assignability goes through the base constraint and succeeds upstream; TSR's relation fails and substitutes string.
- **tsgo:** internal/checker/checker.go:7976 checkTemplateExpression (templateConstraintType test, +13)
- **TSR:** crates/tsr-checker/src/templates.rs template expression span typing / relater.rs type-parameter constraint of alias-instantiated intersection
- **Example:** `conformance/templateLiteralTypes6` [0:13] ``${scope}:${event}``. want ``${Scope}:${Event}``, got ``${string}:${string}``
- **Wave-1 evidence:** templateLiteralTypes6: `${scope}:${event}` want `${Scope}:${Event}` got `${string}:${string}`; p17.ts: `S extends Keyof<R>` -> `${string}:`, `S extends keyof R & string` -> `${S}:`
- **Cases:** conformance/templateLiteralTypes6 [type-operators]

## Cases (lines) that could not be attributed

| case | positions | what was eliminated / notes |
|---|---|---|
| compiler/contextualTypingOfTooShortOverloads | 0:12, 0:16 | Minimal intersection callee H3&H4 with rest-param overloads and context-sensitive arrow resolves correctly in TSR (H3 & H4); the fixture's IRouterHandler<this>/IRouterMatcher<this> polymorphic-this is the untested difference. Not reorder/arity/failure-candidate. (other clusters in case: none) |
| compiler/correlatedUnions | 0:119, 0:142, 0:145, 0:218, 0:219, 0:232, 0:233, 0:248, 0:249, 0:250, 0:280, 0:301, 0:314, 0:320, 0:439, 0:448, 0:449, 0:454, 0:469, 0:470, 0:475 | mixed: block=>.. errors via type-param gate; funs mapped not resolved; MappedFromOriginal realias; Keys/KeyOfOriginal keyof-alias printing, createEventListener & assertedCheck not diagnosed (other clusters in case: FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS, LITERAL-WIDENING, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS) |
| compiler/declFileGenericType2 | 0:1, 0:13, 0:22, 0:36, 0:37, 0:47, 0:49, 0:6 | `mvc`/`composite` namespace refs want any (errors baseline: unresolved/uninstantiated namespace value) got typeof mvc; not investigated within budget (other clusters in case: none) |
| compiler/declarationEmitClassMixinLocalClassDeclaration | 0:11 | `class X extends base` (base: AnyConstructor<Base,...>) line wants instance Base (heritage); Mixin(...) call error not diagnosed (other clusters in case: CLASS-GET-BASE-TYPES) |
| compiler/discriminantPropertyCheck | 0:346 | MapOfAllTests = Record<string, AllTests> keeps alias via re-attach. 0:346 test narrowed to TestA \| TestB (alias dropped by narrowing); not verified (budget). (other clusters in case: TYPE-ALIAS-INSTANTIATION-NEW-ALIAS) |
| conformance/dependentDestructuredVariablesFromNestedPatterns | 0:53, 0:54, 0:56, 0:57, 0:58 | p1 dependent-destructuring flow; mapped union order; generic homomorphic mapped type expanded; promises.map apparent type (result/tuples downstream); as-const tuple mutable in tsgo (other clusters in case: APPARENT-MAPPED-ARRAY-CONSTRAINT, DESTRUCTURE-FLOW-DEPENDENT, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, MAPPED-INSTANTIATE-HOMOMORPHIC-ARMS) |

## Cases with unaligned lines (no cluster can finish them alone)

In these cases the verdict could not align some baseline lines: the expression text differs, or the baseline line spans several lines. Those lines are not in any count above.

| case | ~unaligned | clusters on aligned lines |
|---|---|---|
| compiler/contextualParamTypeVsNestedReturnTypeInference2 | 4 | CONDITIONAL-INLINE-NODE-INSTANTIATION, MERGED-FUNCTION-INTERFACE |
| compiler/contextualParamTypeVsNestedReturnTypeInference3 | 4 | CONDITIONAL-INLINE-NODE-INSTANTIATION, MERGED-FUNCTION-INTERFACE |
| compiler/contextualParamTypeVsNestedReturnTypeInference4 | 1 | INFER-NO-CANDIDATE-GUARD, MERGED-FUNCTION-INTERFACE |
| compiler/contextualSignatureConditionalTypeInstantiationUsingDefault | 1 | CONDITIONAL-INLINE-NODE-INSTANTIATION |
| compiler/contextualTypeBasedOnIntersectionWithAnyInTheMix5 | 1 | TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS |
| compiler/correlatedUnions | 1 | FUNCEXPR-GROUNDED-GATE-OUTER-TYPEPARAMS, LITERAL-WIDENING, MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS, UNATTRIBUTED |
| compiler/declarationEmitMappedTypePreservesTypeParameterConstraint [type-operators] | 3 | DEFERRED-TYPE-REFERENCE-ALIAS, INSTANTIATE-MAPPED-TYPE-ALIAS-DROP, RELATER-NONPRIMITIVE-INDEX-SIGNATURE, SIGNATURE-DECLARATION-ANNOTATION-REUSE, TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION |
| compiler/declarationEmitShadowingInferNotRenamed | 1 | MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT |
| compiler/deferredConditionalTypes | 1 | CONDITIONAL-DEFERRAL-GATE, MAPPED-AS-CLAUSE-INDEX-TYPE |
| compiler/divideAndConquerIntersections | 1 | CONDITIONAL-TYPE-ROOT-ALIAS, PREFIX-NOT-TYPE-FACTS, TYPE-PARAMETER-CONSTRAINT-NODE-REUSE |
| compiler/genericConditionalConstrainedToUnknownNotAssignableToConcreteObject | 2 | RELATE-CONDITIONAL |
| compiler/homomorphicMappedTypeWithNonHomomorphicInstantiationSpreadable1 | 1 | ARG-FINAL-CONTEXT-RECHECK |
| compiler/indexedAccessAndNullableNarrowing | 1 | NARROWABLE-REF-INFERENTIAL-GATE, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS |
| compiler/indexingTypesWithNever | 1 | TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION, TYPE-ALIAS-INSTANTIATION-NEW-ALIAS |
| compiler/intersectionConstraintReduction | 3 | TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION |
| compiler/jsxComplexSignatureHasApplicabilityError | 3 | CLASS-GET-BASE-TYPES, QUALIFIED-NAME-LEFT-ALIAS-RESOLVE, QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE |
| compiler/mappedTypeTupleConstraintAssignability [type-operators] | 1 | MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT |
| compiler/reverseMappedTypeContextualTypesPerElementOfTupleConstraint | 1 | CONTEXTUAL-MAPPED-TUPLE-CONSTRAINT |
| compiler/unionReductionWithStringMappingAndIdenticalBaseTypeExistsNoCrash | 1 | SYNTHETIC-DEFAULT-IMPORT-TARGET |
| conformance/generatorYieldContextualType | 1 | APPEND-LOCAL-TYPE-PARAMETERS, CONDITIONAL-BRANCH-NAMED-UNION, CONTEXTUAL-RETURN-GENERATOR-FILTER, GENERATOR-ANNOTATION-ITERATION-TYPES, INDEXED-ACCESS-GENERIC-DEFER, ORIGIN-ENTRY-ORDER, REMOVE-SUBTYPES-UNDECIDABLE, YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE |
| conformance/importAttributes11 | 3 | IMPORT-ATTRIBUTES-ALIAS-GATE |
| conformance/reExportAliasMakesInstantiated | 7 | MODULE-INSTANCE-STATE-EXPORT-ALIAS |
| conformance/templateLiteralTypes6 [type-operators] | 2 | TEMPLATE-SPAN-CONSTRAINT-ASSIGNABILITY |

## Dependencies and pitfalls reported by verifiers

- **CLASS-BASE:** port `getBaseTypes`/`resolveBaseTypesOfClass`/`getBaseConstructorTypeOfClass` once in the checker. The producer heritage arms (`types_producer.rs:752`, `:881`) then become `getTypeOfNode(EWTA)` plus the writer fallback. Members, `super`, the static side and the mixin class type are separate migrations onto the same function. Four heritage cases also need `APPEND-LOCAL-TYPE-PARAMETERS`, `LITERAL-OF-CONTEXTUAL-TYPEVAR` or `INFER-NO-CANDIDATE-GUARD`.
- **TYPE-ALIAS attribute:** TSR types carry no alias attribute. `TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION`, `TYPE-ALIAS-INSTANTIATION-NEW-ALIAS` and `DEFERRED-TYPE-REFERENCE-ALIAS` all need it. `INSTANTIATE-MAPPED-TYPE-ALIAS-DROP` and `CONDITIONAL-TYPE-ROOT-ALIAS` depend on the core arm. Upstream's newAlias result depends on the referenced body's shape (oracle: `Partial`/`Identity` keep the inner name, `Pick`/`Record`/`Omit` take the outer). `correlatedUnions` also needs constraint-node reuse.
- **QUALIFIED type references:** `QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE` and `IMPORT-EQUALS-ALIAS-TYPEREF-TARGET` remove print mints that stood in for symbol-chain qualification. TSR qualifies interfaces in its printer but not type aliases (probe: TSR `O`, tsgo `N.O`), so NB alias qualification has to land with them. Land `QUALIFIED-NAME-LEFT-ALIAS-RESOLVE` first. `TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL` prints the alias name only in files with an error baseline (`type_symbol_baseline.go:380`).
- **SYNTHETIC-DEFAULT-IMPORT-TARGET:** once a default alias resolves to the module, tsgo prints the other aliases by the first alias (`import * as M` prints `typeof N`). Currently-RIGHT `typeof M` lines can flip unless alias selection matches upstream. The node-mode arms need the implied node format exposed on `ModuleHost` (`loader.rs:1241` already computes it).
- **Generators:** build one iteration-types engine (`getIterationTypesOfIterable` sync and async, `getIterationTypesOfIterator`, `getIterationTypesOfGeneratorFunctionReturnType`). `checkYieldExpression`, `checkAndAggregateYieldOperandTypes`/`createGeneratorType` and `getContextualReturnType` sit on top of it. The TSR `yield*` Generator arm (`expressions.rs:3632-3653`) is dead because `global_type_symbol` only matches arity-1 globals. `yield* []` is `any` because of `BuiltinIteratorReturn` under `strict: false`, not because the array is degenerate. `CONTEXTUAL-RETURN-IIFE-ARM` and `CONTEXTUAL-RETURN-GENERATOR-FILTER` are arms of the same `getContextualReturnType`; port them together.
- **Mapped/conditional written-text mints:** remove the mapped and conditional text mints first (`declared.rs:447`, `mapped.rs:56`, `signatures.rs:5301`). `MAPPED-INLINE-NODE-INSTANTIATION` and the as-clause arms depend on that. Once `CONDITIONAL-INLINE-NODE-INSTANTIATION` lands, re-probe `Eq<1,1>` for `CONDITIONAL-GENERIC-SIGNATURE-EXTENDS`.
- **NB type-node reuse:** upstream gates reuse on `pseudoTypeEquivalentToType` / type equality. An earlier blanket-reuse attempt regressed (§137, −270), so port the equivalence check, not "reuse whenever an annotation exists". Seven former UNION-ORDER cases live here: the expected order is the written declaration order, which the oracle confirmed for lib signatures and `as` expressions.
- **UNION-ORDER:** tsgo always sorts with `CompareTypes`; type id is only the final tie-breaker. TSR's `unions.rs` also sorts with `compare_types`. The remaining mismatches come from `COMPARE-TYPES-ARMS` (intersection arm, alias-name arm), `ORIGIN-ENTRY-ORDER`, written-text mints, and node reuse.
- **Contextual function-expression gates:** removing the "grounded" gate (`signatures.rs:6201`) previously produced 138 new wrong lines in `generatedContextualTyping`. Port `CONTEXTUAL-SIGNATURE-TYPEPARAM-ADOPTION` in the same change.
- **UNTYPED-CALL:** widening the callee gate does not fix argument lines. `xx(a => a)` with a written `any` callee still gaps the arrow. `UNTYPED-CALL-ARGUMENT-ANY-CONTEXT` is a separate fix in `contextual.rs:2446`.

## Caveats

- Wave-2 line maps placed a line that needs two fixes under one cluster, normally the later one in fix order. In those cases "finished alone" is optimistic by the size of the hidden co-blocker. Verifiers named the known instances in their reports (e.g. `renamingDestructuredPropertyInFunctionType` alias lines, `generatorYieldContextualType` 0:13, `internalImport*` needing NB naming).
- Near-duplicate fixtures (React-lib `*react16*` variants, `*_es6`/`_es2017` pairs, `collisionThis*`) inflate blocked/finished counts without adding independent witnesses. Verifiers flagged them inside clusters.
- Counts are attributable to tree `d74b5402` plus the untracked files present at 00:41. Remote lanes landing later will move them.
