# r6-triage — fresh root-cause queue for `checker_types` and diagnostics (`tsr-2zk.1144`)

Round-6 triage box on epic `tsr-2zk`. The `.16.x` root clusters
(`docs/parity/types-triage.md`, `types-triage-2.md`) and the diagnostics
clusters were cut on older bases, and round 6 has moved several hundred lines
since. This note re-clusters every failing line and case on the current
integration tip by **native root cause**, ranks the clusters by cases each
would convert alone, and marks the unowned ones. §4 ports the top unowned
single-operation cluster. Native source is `vendor/typescript-go` @ `5b1047d`.
Proposed Beads entries are in [`r6-triage-issues.json`](r6-triage-issues.json)
(`bd` is unavailable in boxes; the integrator files them).

## 0. Base and setup

Frozen base: `claude/beautiful-shannon-ar5gh0` @ `e6eadf4` (batch BL plus
the snapshot refresh).

- types: 556,303 aligned lines, 550,221 RIGHT / 5,324 WRONG / 758 GAP;
  6,082 non-RIGHT lines in 1,174 cases;
- diagnostics: 12,238 cases, 5,612 RIGHT / 5,606 EMPTY_RIGHT / 981 WRONG /
  39 EMPTY_WRONG; 1,020 non-RIGHT cases (the brief's "about 810" predates
  the configured-variant rows);
- Ir (`valgrind --tool=callgrind`, release `tsr -p … --noEmit
  --singleThreaded true --pretty false`): domain-model 1,092,484,408;
  generic-imports 343,109,097.

Setup as r5-operators3 §4: PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept in the session
scratchpad (not committed). The native oracle is `build-tsgo.sh`'s `tsgo`
(Go 1.26.8, `Version 7.1.0-dev`).

Dump hygiene, for the next box: `TSR_VERDICT_EXPR=1` rows carry expression
text with raw bytes, so `grep` treats `types.tsv` as binary and silently
truncates (the first loss check here printed nothing for that reason). Use
`grep -a`, or compare the non-expression dump. Python readers must open the
file with `newline='\n'`: an expression holding `\r` splits a row otherwise.

## 1. Method

1. Both suites were dumped unfiltered, alone (`verdictdump` with
   `TSR_VERDICT_EXPR=1`, `diagverdictdump`).
2. For diagnostics each case's sorted multiset difference gives its missing
   (`-CODE`), extra (`+CODE`) and moved (`~CODE`, same code at another
   position) diagnostics.
3. Nine classifiers took the population in path order: six types packets of
   about 1,010 lines (≈195 cases) and three diagnostics packets of 340 cases.
   Each keyed every line or case to the upstream operation whose faithful
   port converts it, reusing the `types-triage-2.md` vocabulary (411 keys)
   and the round-6 notes' routed causes where the operation was the same.
   They checked the large clusters against `tsgo` and `probefile` with cut-down
   repros; each record's `verified` field names its sample.
4. Synonymous keys minted in different packets were folded (for example
   `PARSE-ERROR-SEMANTIC-GATE` and `PARSE-ERROR-FILE-CHECK-DECLINE(S)`; three
   call-applicability keys into `CALL-ARGUMENT-APPLICABILITY-REPORT`; two
   TS2769 keys into `OVERLOAD-FAILURE-REPORT`). After folding there are 759
   clusters. Every line and case is assigned. Two type lines
   (`trackedSymbolsNoCrash`) are `UNCLASSIFIED`.
5. **Finished alone** is recomputed from the per-line map, not taken from
   the classifiers. A types case counts when every one of its non-RIGHT
   lines is in the cluster. A diagnostics case counts when the cluster is
   the only one owning a differing diagnostic. **Unlocked** is the sum of
   the two counts, which is how the table is ranked. 1,024 of 1,174 types
   cases and 962 of 1,020 diagnostics cases sit in a single cluster.
   Unaligned type lines are not in the dump. A case with one would also need
   its walker fixed, so a types count can overstate by the few cases that
   have both (`types-triage-2.md` counted 86 such cases at `05a05dc3`).
6. Owner is the lane holding the file where the faithful port lands, per the
   round-6 ownership table: MAIN's claims and active lanes, the r6 boxes,
   then UNOWNED.

## 2. Ranked table (clusters unlocking ≥3 cases)

Columns:
- **unlocked**: types + diagnostics cases finished alone.
- **types**: finished alone / cases touched / non-RIGHT lines.
- **diag**: finished alone / cases touched.
- **dispatch**: **yes** means unowned and a single native operation, so it
  is safe to hand to a box as new files plus hook diffs.

`(family)` marks a row the classifier grouped by area rather than by one
native operation. Split it before dispatch (§3).

| # | cluster | unlocked | types | diag | native (pinned) | port | owner | dispatch |
|---|---|---|---|---|---|---|---|---|
| 1 | `PARSE-ERROR-FILE-CHECK-DECLINE` | 43 | 0 / 0 / 0 | 43 / 55 | none upstream: checker/checker.go reports unconditionally (resolveCallExpression :8471, resolveTaggedTemplateExpression :8719, reportNonexistentPrope… | {check.rs,calls.rs,nonexistent_property.rs,type_argument_arity.rs,...}: `if self.file_has… | MAIN |  |
| 2 | `CALL-ARGUMENT-APPLICABILITY-REPORT` | 36 | 0 / 0 / 0 | 36 / 39 | checker/checker.go:9256 isSignatureApplicable (+ resolveCall:8843 reportErrors pass) | calls.rs::check_single_candidate_arguments / check_call_arity (CallArity::ApplicableGener… | MAIN |  |
| 3 | `TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION` | 34 | 34 / 60 / 323 | 0 / 0 | checker/checker.go:23837 getDeclaredTypeOfTypeAlias; :23641 getTypeAliasInstantiation; :22104 instantiateTypeWithAlias; :23580 getTypeFromTypeAliasRe… | declared.rs:6667 get_declared_type_of_type_alias; :4397 get_instantiated_type_reference; … | r6-declared2 |  |
| 4 | `SPECIFIER-FOR-MODULE-SYMBOL/NODE-MODULES` | 30 | 26 / 31 / 122 | 4 / 4 | modulespecifiers/specifiers.go:405-423 computeModuleSpecifiers node_modules arm, :743 tryGetModuleNameAsNodeModule (exports conditions / typesVersion… | module_specifiers.rs (node_modules arm) | r6-specifiers |  |
| 5 | `NONEXISTENT-PROPERTY-CERTIFICATION-GATE` | 24 | 0 / 0 / 0 | 24 / 28 | checker/checker.go:11258 checkPropertyAccessExpressionOrQualifiedName -> :11530 reportNonexistentProperty (no certification step) | nonexistent_property.rs::report path (receiver_type_is_the_declared_one, property_is_know… | MAIN |  |
| 6 | `GET-SYMBOL-CHAIN-NEEDS-QUALIFICATION/EXPORT-SPECIFIER-NOT-IN-SCOPE` | 23 | 23 / 30 / 80 | 0 / 0 | checker/symbolaccessibility.go:573 trySymbolTable (export-specifier aliases excluded from local lookup) | checker.rs::symbol_chain imported_here gate | MAIN |  |
| 7 | `CANDIDATE-FOR-OVERLOAD-FAILURE` | 22 | 13 / 15 / 55 | 9 / 11 | checker/checker.go:8843 resolveCall (candidatesForArgumentError / 2769 reporting after :9025 chooseOverload fails; explicit type-argument candidate f… | calls.rs (overload resolution report) | MAIN |  |
| 8 | `IMPLICIT-ANY-PARAMETER-REPORT` | 20 | 0 / 0 / 0 | 20 / 20 | checker/checker.go:18242 widenTypeForVariableLikeDeclaration -> reportImplicitAny 18275 (reached from getTypeOfParameter/assignContextualParameterTyp… | implicit_any.rs::check_implicit_any_parameters / contextual_parameter_type_is_absent | MAIN |  |
| 9 | `ARG-CONTEXT-RESOLVED-SIG` | 19 | 18 / 31 / 94 | 1 / 1 | checker/checker.go:29772 getContextualTypeForArgumentAtIndex (getResolvedSignature :29789) + :25522 isLiteralOfContextualType | contextual.rs::contextual_type_for_argument | MAIN |  |
| 10 | `SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS` | 19 | 19 / 26 / 75 | 0 / 0 | checker/symbolaccessibility.go getAccessibleSymbolChain -> getAccessibleSymbolChainFromSymbolTable (import-equals alias arm) | checker.rs symbol_chain (local import-equals alias not admitted for module symbols) | MAIN |  |
| 11 | `RESOLVE-ALIAS-INDIRECTION` | 18 | 18 / 24 / 67 | 0 / 0 | checker/checker.go:16266 resolveAlias (:16293 resolveIndirectionAlias); :24094 getDeclaredTypeOfAlias | symbols.rs::resolve_alias (one hop) / resolve_alias_fully | MAIN |  |
| 12 | `TAGGED-TEMPLATE-EFFECTIVE-ARGS` | 17 | 13 / 13 / 61 | 4 / 4 | checker/checker.go:8719 resolveTaggedTemplateExpression -> :30047 getEffectiveCallArguments (tagged arm) -> :8843 resolveCall (chooseOverload, getCan… | calls.rs:3119 check_tagged_template_expression | MAIN |  |
| 13 | `JSX-OVERLOAD-AND-COMPONENT-REPORT` (family) | 17 | 2 / 2 / 16 | 15 / 15 | checker/jsx.go:130 checkJsxOpeningLikeElementOrOpeningFragment -> resolveJsxOpeningLikeElement overload failure (2769), component validity (2786/2607… | jsx_component.rs | r6-jsx |  |
| 14 | `JS-FILE-CHECK-DECLINE` | 17 | 0 / 0 / 0 | 17 / 18 | checker/checker.go:9760 getArgumentArityError / 9279 isSignatureApplicable (resolveCall), 10808 checkDeleteExpression, 11428 getFlowTypeOfAccessExpre… | call_arity.rs::check_call_arity, calls.rs::check_resolve_call_arity (in_js_file gates), i… | MAIN |  |
| 15 | `PRIVATE-IDENTIFIER-GRAMMAR` (family) | 17 | 1 / 1 / 1 | 16 / 16 | checker/grammarchecks.go:100 checkGrammarPrivateIdentifierExpression; checker.go:11494 checkPrivateIdentifierPropertyAccess; grammarchecks checkGramm… | unported (no private-identifier grammar pass) | UNOWNED | split first |
| 16 | `INFER-NO-CANDIDATE-GUARD` | 16 | 14 / 23 / 106 | 2 / 4 | checker/inference.go:1317 getInferredType (no-candidate arm :1358-1377, constraint arm :1378-1400) | inference.rs::check_generic_call_worker (structural_source_supplied decline) | MAIN |  |
| 17 | `GET-SYMBOL-CHAIN-NEEDS-QUALIFICATION` | 16 | 16 / 22 / 53 | 0 / 0 | checker/nodebuilderimpl.go:1087 getSymbolChain -> symbolaccessibility.go getAccessibleSymbolChain (needs qualification) | checker.rs symbol_chain | MAIN |  |
| 18 | `CALL-SPREAD-ARGUMENT-APPLICABILITY` | 16 | 0 / 0 / 0 | 16 / 16 | checker/checker.go:29500 getSpreadArgumentType / getEffectiveCallArguments synthetic spread elements checked in getSignatureApplicabilityError | calls.rs (spread arguments) | MAIN |  |
| 19 | `SHADOWED-TYPEPARAM-RENAME` | 15 | 15 / 18 / 64 | 0 / 0 | checker/nodebuilderimpl.go:1404 typeParameterToName; :1396 typeParameterShadowsOtherTypeParameterInScope | signatures.rs:7005 signature_to_string_at (:7010 predicate bail, :7023 site_anchor = sign… | r6-lazytext |  |
| 20 | `PARSER-RECOVERY-DIVERGENCE` (family) | 15 | 0 / 0 / 0 | 15 / 21 | parser/parser.go (statement/expression recovery: parseExpressionOrLabeledStatement :1516, parseTypeAssertion :5132, parseImportCall/Attributes :3085,… | crates/tsr-parser | MAIN |  |
| 21 | `OVERLOAD-FAILURE-REPORT` | 15 | 0 / 0 / 0 | 15 / 17 | checker/checker.go:9649 reportCallResolutionErrors (candidatesForArgumentError / TS2769 chain) | calls.rs (non_generic_overload_candidates walk ~1180-1265, declines on object/array liter… | MAIN |  |
| 22 | `RESOLVE-DECORATOR-CALL-ERRORS` | 15 | 0 / 0 / 0 | 15 / 15 | checker/checker.go:8743 resolveDecorator (+ checkDecorator:6061, getDiagnosticHeadMessageForDecoratorResolution:8789) | unported (calls.rs has no decorator call resolution) | MAIN |  |
| 23 | `SCANNER-NUMERIC-AND-ESCAPE-DIAGNOSTICS` (family) | 15 | 0 / 0 / 0 | 15 / 15 | scanner/scanner.go:2151 scanBinaryOrOctalDigits, :1690 scanEscapeSequence, :1944 scanNumber; scanner/regexp.go (regex escape/flag checks); keyword es… | crates/tsr-parser scanner | r6-lazytext |  |
| 24 | `ARRAY-LITERAL-TUPLE-CONTEXT` | 14 | 9 / 15 / 69 | 5 / 7 | checker/checker.go:8021 checkArrayLiteral (:8029 inTupleContext), :23544 isTupleLikeType | array_literals.rs:631 array_literal_has_a_tuple_contextual_type (instantiate_contextual_i… | MAIN |  |
| 25 | `CHECK-GRAMMAR-MODIFIERS` (family) | 14 | 0 / 0 / 0 | 14 / 15 | checker/grammarchecks.go:214 checkGrammarModifiers (+ checkGrammarProperty:1894 TS1276) | grammar.rs (partial; no TS1029/TS1243/TS1275/TS1207/TS1495/TS8038 emitter) | MAIN |  |
| 26 | `IMPLICIT-ANY-AUTO-TYPED-VARIABLE` | 13 | 0 / 0 / 0 | 13 / 14 | checker/checker.go:11042 checkIdentifier auto-type arm (TS7034 at declaration, TS7005 at reference) / reportImplicitAny widening (TS7005 '[any, any]'… | implicit_any.rs | MAIN |  |
| 27 | `JSDOC-TAG-SEMANTICS` (family) | 13 | 0 / 0 / 0 | 13 / 14 | checker/checker.go JSDoc @template/@overload/@type/@typedef/@import handling (template defaults 2706/2744, overload compatibility 2394, typedef dupli… | jsdoc_*.rs | r6-jsdoc |  |
| 28 | `SPREAD-PROPERTY-ANNOTATION-REUSE` | 12 | 12 / 13 / 25 | 0 / 0 | checker/checker.go getSpreadSymbol + nodebuilderimpl.go addPropertyToElementList -> serializeTypeForDeclaration | spreads.rs (printing hunk r6-nodereuse-property-slot-spreads.diff) | r6-errorsplit2 (held r6-nodereuse-prope… |  |
| 29 | `MODULE-RESOLUTION-DIAGNOSTICS` (family) | 12 | 0 / 0 / 0 | 12 / 12 | checker/checker.go:15149 resolveExternalModule (7016 untyped, 2877 non-relative .ts rewrite, 1543/1544 JSON in ESM, 2665 untyped augmentation, 1192 c… | crates/tsr-compiler (resolution) + crates/tsr-checker module_format.rs | r6-modules2 |  |
| 30 | `RESOLVE-ES-MODULE-SYMBOL-CLONE` | 11 | 11 / 11 / 208 | 0 / 0 | checker/checker.go:15568 resolveESModuleSymbol (isEsmCjsRef / default-property arm :15610-15618 -> cloneTypeAsModuleType :15721) | symbols.rs::module_clone_type | MAIN |  |
| 31 | `CONTEXTUAL-BINDING-PATTERN-INITIALIZER` | 11 | 10 / 12 / 96 | 1 / 1 | checker/checker.go getContextualTypeForInitializerExpression (binding-pattern arm -> getTypeFromBindingPattern(includePatternInType)) | contextual.rs (initializer contextual type) | MAIN |  |
| 32 | `TYPE-ALIAS-ACCESSIBILITY-GATE` | 11 | 11 / 15 / 70 | 0 / 0 | checker/nodebuilderimpl.go:3362 typeToTypeNode alias arm -> symbolaccessibility.go:11 IsTypeSymbolAccessible | checker.rs alias_of print arm (IsTypeSymbolAccessible landed in 64a4a5c but still admits … | MAIN (.16.2/.39 printer) |  |
| 33 | `TYPE-ALIAS-INSTANTIATION-NEW-ALIAS` | 11 | 11 / 19 / 57 | 0 / 0 | checker/checker.go:23580 getTypeFromTypeAliasReference (newAliasSymbol 23606-23628) -> :23641 getTypeAliasInstantiation | declared.rs::get_instantiated_type_reference | r6-declared2 |  |
| 34 | `PARSER-REPARSE-TOP-LEVEL-AWAIT` | 11 | 4 / 4 / 20 | 7 / 7 | parser/parser.go:514 reparseTopLevelAwait | crates/tsr-parser (unported) | MAIN |  |
| 35 | `TRY-SYMBOL-TABLE-DEFAULT-IMPORT-ALIAS` | 11 | 11 / 16 / 26 | 0 / 0 | checker/nodebuilder trySymbolTable (default-import alias of export= module names `typeof React`) | symbol_access.rs | MAIN |  |
| 36 | `BINDING-PATTERN-IMPLIED-TYPE` | 11 | 9 / 9 / 22 | 2 / 2 | checker/checker.go:16790 getTypeForVariableLikeDeclaration (pattern arm -> :17904 getTypeFromBindingPattern) | destructure.rs (parameter pattern type gate) | MAIN |  |
| 37 | `CONDITIONAL-INLINE-NODE-INSTANTIATION` | 10 | 10 / 24 / 167 | 0 / 0 | checker/checker.go:22485 getConditionalTypeInstantiation (anonymous conditional root from a non-alias ConditionalTypeNode) | declared.rs::instantiate_conditional_node (in_constraint gate) / evaluate_conditional_node | r6-declared2 |  |
| 38 | `SYMBOL-CHAIN-CANDIDATE-EXPORTS-OF-SYMBOL` | 10 | 10 / 10 / 38 | 0 / 0 | checker/symbolaccessibility.go:620 getCandidateListForSymbol (via :535 trySymbolTable, isLocalNameLookup=false keeps `export * as ns` re-exports) | checker.rs::best_name / symbol_chain | MAIN |  |
| 39 | `TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL` | 10 | 10 / 10 / 23 | 0 / 0 | checker/checker.go getTypeFromTypeReference -> errorType carrying the unresolved alias name | declared.rs (type reference to unresolved import) | r6-declared2 |  |
| 40 | `GRAMMAR-MISC-DECLARATIONS` (family) | 10 | 0 / 0 / 0 | 10 / 11 | checker/grammarchecks.go (override in ambient 1040, labels 1344, parameter list 2370/1092/1098, optional binding parameter 2463, rest param array 237… | unported (grammar.rs) | MAIN |  |
| 41 | `REGEXP-SCANNER-VALIDATION` | 10 | 0 / 0 / 0 | 10 / 10 | scanner/regexp.go scanRegularExpressionWorker/run (+ scanner.go:1180 ReScanSlashToken flags, scanEscapeSequence 1487/1535-1538) | unported (tsr-scanner has no regular-expression body/flag validator) | r6-lazytext |  |
| 42 | `PARSER-IS-YIELD-EXPRESSION` | 10 | 2 / 2 / 2 | 8 / 8 | parser/parser.go:4150 isYieldExpression | tsr-parser/src/expression.rs::parse_assignment_expression_worker | MAIN |  |
| 43 | `MODULE-AUGMENTATION-MERGE` | 9 | 9 / 12 / 41 | 0 / 0 | checker/checker.go:1397 mergeModuleAugmentation (export= target merged/cloned) + :14390 getSymbolOfDeclaration | crates/tsr-binder (augmentation merge) / symbols.rs merged_symbol | MAIN |  |
| 44 | `CONTEXTUAL-SIGNATURE-TYPEPARAM-ADOPTION` | 9 | 9 / 10 / 33 | 0 / 0 | checker/checker.go:10349 assignContextualParameterTypes (adoption :10350-10356) | signatures.rs::get_signature_from_declaration | MAIN |  |
| 45 | `CHECK-GRAMMAR-AWAIT-YIELD-CONTEXT` | 9 | 0 / 0 / 0 | 9 / 10 | checker/grammarchecks.go:1676 checkGrammarAwaitOrAwaitUsing (+ checker.go:4038 checkForOfStatement TS18038, grammarchecks.go:1780 checkGrammarYieldEx… | grammar.rs / check.rs (TS1308 partial; TS18037/18038/18054/2852/2853 unported) | MAIN |  |
| 46 | `IMPORT-ATTRIBUTES-CHECKS` (family) | 9 | 0 / 0 / 0 | 9 / 9 | checker/checker.go:5408 checkImportAttributes (2322 vs ImportAttributes/ImportCallOptions, 2858 non-literal values, 2823/2857 module/type-only gates)… | import_attributes.rs | UNOWNED | ported here (§4) |
| 47 | `ARRAY-LITERAL-OMITTED-EXPRESSION-ELEMENT` | 8 | 8 / 8 / 38 | 0 / 0 | checker/checker.go:8021 checkArrayLiteral (default arm :8071, inDestructuringPattern) | array_literals.rs check_array_literal | MAIN |  |
| 48 | `RESOLVE-NAME-FUNCTION-LIKE-PARAMETER-SCOPE` | 8 | 5 / 5 / 34 | 3 / 3 | binder/nameresolver.go:54 Resolve (function-like locals arm: body locals invisible from parameter initializers) | crates/tsr-binder/src/lib.rs:916 parameter scope handling | r6-names2 |  |
| 49 | `IMPORT-TYPE-NODE-TYPEOF` | 8 | 8 / 12 / 36 | 0 / 0 | checker/checker.go:24575 getTypeFromImportTypeNode (IsTypeOf arm :24608-24609, :24660-24662) | declared.rs::get_type_from_import_type_node | r6-declared2 |  |
| 50 | `OBJLIT-THIS-LITERAL-SELF-FALLBACK` | 8 | 7 / 9 / 33 | 1 / 1 | checker/checker.go:12021 getContextualThisParameterType (fallback :12049-12054) | contextual.rs:611 contextual_object_this_type (decline :634-644 object_literal_member_ret… | MAIN |  |
| 51 | `IMPORT-TYPE-NODE-TYPE-MEANING` | 8 | 5 / 8 / 28 | 3 / 3 | checker/checker.go:24575 getTypeFromImportTypeNode | declared.rs::get_type_from_import_type_node | r6-typesroots2 |  |
| 52 | `CONSTRAINT-OF-TYPE-PARAMETER-CIRCULARITY` | 8 | 6 / 6 / 18 | 2 / 2 | checker/checker.go:17059 getConstraintOfTypeParameter (:17066 hasNonCircularBaseConstraint) | signatures.rs:5819 type_parameter_of | MAIN |  |
| 53 | `GRAMMAR-ACCESSOR-DECLARATION` | 8 | 0 / 0 / 0 | 8 / 9 | checker/grammarchecks.go:1307 checkGrammarAccessor | unported (grammar.rs) | MAIN |  |
| 54 | `CHECK-INTERFACE-HERITAGE-AND-BASES` (family) | 8 | 0 / 0 / 0 | 8 / 9 | checker/checker.go:4991 checkInterfaceDeclaration (heritage checkTypeReferenceNode, 2312/2430/2411/2413/2320/2499/2428) and :4293 checkClassLikeDecla… | heritage_conformance.rs / check.rs | MAIN |  |
| 55 | `PARSER-FUNCTION-BLOCK-OR-SEMICOLON-RECOVERY` | 8 | 0 / 0 / 0 | 8 / 8 | parser/parser.go:3481 parseFunctionBlockOrSemicolon (and method `?` / `=>` recovery in class/object members) | crates/tsr-parser | MAIN |  |
| 56 | `CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES` | 7 | 6 / 7 / 56 | 1 / 1 | checker/checker.go:19127-19152 resolveObjectTypeMembers (base merge) over :19167 getBaseTypes / resolveBaseTypesOfClass | members.rs::get_property_of_declared_symbol (base walk) | MAIN |  |
| 57 | `DISCRIMINATE-CONTEXTUAL-BY-MEMBERS` | 7 | 7 / 8 / 45 | 0 / 0 | checker/checker.go:30755 discriminateContextualTypeByObjectMembers; relater.go:1212 discriminateTypeByDiscriminableItems | symbols.rs::discriminate_union_root | MAIN |  |
| 58 | `IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET` | 7 | 7 / 7 / 31 | 0 / 0 | checker/checker.go:14474 getSymbolOfPartOfRightHandSideOfImportEquals (via 14439 getTargetOfImportEqualsDeclaration; resolveEntityName 15772) | symbols.rs:1124 resolve_alias ModuleReference::Identifier arm (ALIAS branch 1148-1165) | MAIN |  |
| 59 | `CONDITIONAL-DEFERRAL-GATE` | 7 | 5 / 7 / 25 | 2 / 2 | checker/checker.go:24300 getConditionalType (:24475 isDeferredType) | declared.rs:7599 evaluate_conditional_node (mentions-type-parameter gate) | r6-declared2 |  |
| 60 | `MERGE-SYMBOL-RESOLVE-ALIAS-TARGET` | 7 | 1 / 3 / 3 | 6 / 6 | checker/checker.go:14109 mergeSymbolTable / mergeSymbol (alias targets resolved before merging re-exported/augmented declarations) | symbols.rs | MAIN |  |
| 61 | `DEFAULT-EXPORT-ALIAS-CLASS-MERGE` | 7 | 7 / 7 / 8 | 0 / 0 | checker/checker.go resolveSymbol / getNameOfSymbolAsWritten for merged export-default symbols | symbols.rs default-export merge | MAIN |  |
| 62 | `MEANING-MISMATCH-REPORTS` | 7 | 0 / 0 / 0 | 7 / 7 | checker/checker.go:1596 onFailedToResolveSymbol -> checkAndReportErrorForUsingTypeAsNamespace (2702/2713), checkAndReportErrorForUsingNamespaceAsType… | meaning_mismatch.rs | r6-modules2 |  |
| 63 | `JSX-COMPONENT-ATTRIBUTES-RESOLUTION` | 7 | 0 / 0 / 0 | 7 / 7 | checker/jsx.go resolveJsxOpeningLikeElement / checkJsxReturnAssignableToAppropriateBound (2786) / checkApplicableSignatureForJsxCallLikeElement 670 (… | jsx_component.rs | r6-jsx |  |
| 64 | `PARSER-DECORATOR-EXPRESSION` | 7 | 0 / 0 / 0 | 7 / 7 | parser/parser.go:3906 parseDecoratorExpression (+ parseDecoratedExpression:5730, grammarchecks.go:127 checkGrammarDecorator TS1497) | tsr-parser/src/expression.rs::parse_decorator_expression | MAIN |  |
| 65 | `LAZY-FUNCEXPR-RETURN-CIRCULARITY` | 6 | 6 / 7 / 91 | 0 / 0 | checker/checker.go:20001 getReturnTypeOfSignature (lazy, own resolution stack); :10114 checkFunctionExpressionOrObjectLiteralMethod | signatures.rs::get_type_of_function_expression / return_type_from_body | MAIN |  |
| 66 | `NARROW-PRIVATE-IDENTIFIER-IN-EXPRESSION` | 6 | 6 / 6 / 32 | 0 / 0 | checker/flow.go:982 narrowTypeByPrivateIdentifierInInExpression (from :469 narrowTypeByBinaryExpression) | flow.rs narrow_type InKeyword arm | MAIN |  |
| 67 | `TRYSYMBOLTABLE-UMD-ALIAS-EXCLUSION` | 6 | 6 / 12 / 27 | 0 / 0 | checker/symbolaccessibility.go:566 trySymbolTable (isUMDExportSymbol && IsExternalModule(enclosing file) exclusion) | checker.rs:3598 module_alias_at (alias loop :3656-3680, no NamespaceExportDeclaration fil… | MAIN |  |
| 68 | `EXTERNAL-MODULE-MEMBER-EXPORT-EQUALS` | 6 | 5 / 5 / 23 | 1 / 1 | checker/checker.go:14667 getExternalModuleMember (export= branch) | symbols.rs (site_independent gate) | MAIN |  |
| 69 | `CONTEXTUAL-ARG-SUPER-CALL` | 6 | 4 / 4 / 18 | 2 / 2 | checker/checker.go:29772 getContextualTypeForArgumentAtIndex -> :8471 resolveCallExpression (super arm) | contextual.rs argument contextual type (no SuperCall callee path) | MAIN |  |
| 70 | `IMPORT-DEFER-CALL-EXPRESSION` | 6 | 6 / 6 / 18 | 0 / 0 | checker/checker.go checkImportCallExpression (import.defer meta-property call) + parser parseImportCall defer arm | calls.rs::check_import_call_expression (no import.defer form) | UNOWNED | **yes** |
| 71 | `JS-DEFAULT-EXPORT-JSDOC-TYPE` | 6 | 6 / 6 / 14 | 0 / 0 | parser/reparser.go:342 reparseHosted (ExportAssignment -> makeNewCast) | symbols.rs:4822 ExportAssignment arm (jsdoc_cast_annotation gate) | r6-jsdoc |  |
| 72 | `PARSER-JSX-IN-JS-UNARY-OPERAND` | 6 | 0 / 0 / 0 | 6 / 6 | parser/parser.go:5060 parseSimpleUnaryExpression / 5132 parseTypeAssertion (LanguageVariantJSX arm) | tsr-parser/src/expression.rs | MAIN |  |
| 73 | `CLASS-PROPERTY-INITIALIZER-CHECKS` (family) | 6 | 0 / 0 / 0 | 6 / 6 | checker/checker.go:4933 checkPropertyInitialization (2564) / checkPropertyAccess used-before-assigned (2565), initializer references to constructor l… | class_fields.rs | UNOWNED | split first |
| 74 | `FLOW-NARROWING-RESULT-TYPE` | 6 | 0 / 0 / 0 | 6 / 6 | checker/flow.go getFlowTypeOfReference narrowing arms (unknown control flow, instanceof generic, keyof (T & {}), declared-type comparisons for 2403) | flow.rs | MAIN |  |
| 75 | `JSX-ATTRIBUTES-RELATION-REPORT` | 6 | 0 / 0 / 0 | 6 / 6 | checker/jsx.go checkJsxAttributes -> checkTypeRelatedToAndOptionallyElaborate on attributes type (LibraryManagedAttributes/Defaultize, IntrinsicAttri… | jsx_attributes.rs | r6-jsx |  |
| 76 | `TYPE-REFERENCE-TO-TYPE-NODE-DEFAULT-ARG-ELISION` | 5 | 5 / 13 / 43 | 0 / 0 | checker/nodebuilderimpl.go:3085 typeReferenceToTypeNode (Iterable-family default elision) + checker.go:23169 fillMissingTypeArguments | declared.rs reference text (reference_display_arity, §136) | r6-declared2 |  |
| 77 | `ORIGIN-SLICE-GATE` | 5 | 5 / 8 / 42 | 0 / 0 | checker/checker.go:25705 getUnionTypeWorker (origin arm) + :26558 filterType origin arm | unions.rs::build_origin_union (slice gate declines non-union OBJECT entries) | UNOWNED (unions.rs; held r6-typesroots-… | **yes** |
| 78 | `GLOBAL-TYPE-MISSING-LIB-FALLBACK` | 5 | 5 / 5 / 39 | 0 / 0 | checker/checker.go:1210 getGlobalType (missing -> emptyObjectType / emptyGenericType) used by :21729 getApparentType (globalBigIntType) and :24701 cr… | members.rs::get_apparent_type (missing global returns id) / array literal creation withou… | MAIN |  |
| 79 | `JSDOC-OVERLOAD-SIGNATURES` | 5 | 4 / 4 / 37 | 1 / 1 | parser/reparser.go:134 reparseUnhosted (KindJSDocOverloadTag arm) | signatures.rs:1358 JS signature-from-JSDoc collection (no @overload arm) | r6-jsdoc |  |
| 80 | `CHOOSE-OVERLOAD-GENERIC-WALK` | 5 | 4 / 8 / 36 | 1 / 1 | checker/checker.go:9025 chooseOverload | calls.rs:3723 choose_ordered_overload walk gate / :4514 transcribed_generic_set_walk | MAIN |  |
| 81 | `GET-THIS-CONTAINER-DECORATOR-ARM` | 5 | 5 / 6 / 31 | 0 / 0 | ast/utilities.go:1790 GetThisContainer (KindDecorator arm) via checker.go:12077 checkThisExpression | expressions.rs::check_this_expression (own ancestor walk lacks the Decorator arm that thi… | UNOWNED | **yes** |
| 82 | `BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS` | 5 | 4 / 4 / 25 | 1 / 1 | checker/checker.go getBindingElementTypeFromParentType (computed-name arm -> getIndexedAccessTypeEx) | destructure.rs binding-element parent indexing (computed name arm) | MAIN |  |
| 83 | `TYPE-PARAMETER-CONSTRAINT-ANY-TO-UNKNOWN` | 5 | 4 / 5 / 24 | 1 / 1 | checker/checker.go:17085 getConstraintFromTypeParameter (any->unknown arm) | members.rs (type parameter constraint) | MAIN |  |
| 84 | `CLASS-TYPE-BASE-TYPE-VARIABLE-INTERSECTION` | 5 | 5 / 5 / 24 | 0 / 0 | checker/checker.go:16912 getTypeOfFuncClassEnumModuleWorker / :16936 getBaseTypeVariableOfClass | symbols.rs:3866 anonymous_class_written_name (parameter decline :3893-3914) | MAIN |  |
| 85 | `IMPORT-CALL-SYNTHETIC-DEFAULT-TYPE` | 5 | 5 / 6 / 24 | 0 / 0 | checker/checker.go:8267 checkImportCallExpression (:8305-8311) -> :15646 getTypeWithSyntheticDefaultImportType / :15707 createDefaultPropertyWrapperF… | calls.rs::check_import_call_expression; module_exports.rs::get_type_with_synthetic_defaul… | MAIN |  |
| 86 | `SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS/ALIAS-OVER-ADMITTED` | 5 | 5 / 5 / 24 | 0 / 0 | checker/symbolaccessibility.go:481 getAccessibleSymbolChainFromSymbolTable (trySymbolTable alias loop; :373 getAccessibleSymbolChain) | symbol_accessibility.rs (alias_in_scope_for) | MAIN |  |
| 87 | `LOGICAL-OR-COALESCE-GENERIC-GATE` | 5 | 5 / 6 / 22 | 0 / 0 | checker/checker.go:12509 checkBinaryLikeExpressionWorker BarBar arm | binary.rs::check_logical_or_coalescing | UNOWNED | **yes** |
| 88 | `GLOBALTHIS-SYMBOL-IN-GLOBALS` | 5 | 5 / 7 / 21 | 0 / 0 | checker/checker.go:962-964 NewChecker (globalThisSymbol), :20674 resolveAnonymousTypeMembers | expressions.rs / members.rs globalThis special cases | MAIN |  |
| 89 | `NARROW-INSTANCEOF-CONSTRUCT-SIGNATURES` | 5 | 5 / 5 / 20 | 0 / 0 | checker/flow.go:811 narrowTypeByInstanceof -> :966 getInstanceType (construct-signature returns) | flow.rs instanceof arm | MAIN |  |
| 90 | `INSTANTIATION-EXPRESSION-TYPE` | 5 | 4 / 7 / 16 | 1 / 1 | checker/checker.go:10660 getInstantiationExpressionType | instantiation_expressions.rs | r6-declared2 |  |
| 91 | `BIND-CLASS-PROTOTYPE-SYMBOL` | 5 | 4 / 4 / 15 | 1 / 1 | binder/binder.go bindClassLikeDeclaration (prototype symbol) -> checker getTypeOfPrototypeProperty | crates/tsr-binder (class prototype symbol) / symbols.rs | MAIN |  |
| 92 | `YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE` | 5 | 5 / 6 / 16 | 0 / 0 | checker/checker.go:20322 checkAndAggregateYieldOperandTypes (nextType from getContextualType(yield)) | signatures.rs::return_type_from_body (yield decline predicate) | r6-lazytext |  |
| 93 | `GET-SYMBOL-CHAIN-NEEDS-QUALIFICATION/NESTED-QUALIFICATION` | 5 | 5 / 8 / 16 | 0 / 0 | checker/nodebuilderimpl.go:1087 getSymbolChain (called per symbolToTypeNode) | checker.rs:2658 qualified_name_at | MAIN |  |
| 94 | `JSDOC-TYPE-TAG-HOSTING` | 5 | 4 / 4 / 13 | 1 / 2 | checker/checker.go getTypeOfSymbol/getSignatureFromDeclaration with @type on function/arrow (closure function types, asserts) | jsdoc_annotations.rs | r6-jsdoc |  |
| 95 | `CHECK-NON-NULL-CALLEE` | 5 | 5 / 6 / 11 | 0 / 0 | checker/checker.go resolveCallExpression (checkNonNullExpression on the callee: reports TS2722 and continues with the non-null type) | calls.rs (call on possibly-undefined callee) | MAIN |  |
| 96 | `IMPORT-TYPE-AND-ALIAS-MEANING` | 5 | 0 / 0 / 0 | 5 / 9 | checker/checker.go:3026-area getTypeFromImportTypeNode / resolveEntityName meaning checks (2694 on import type qualifier, 2749/2702 value-vs-type-vs-… | meaning_mismatch.rs | r6-modules2 |  |
| 97 | `RESOLVE-BASE-TYPES-CIRCULARITY` | 5 | 0 / 0 / 0 | 5 / 6 | checker/checker.go:19220 resolveBaseTypesOfClass / 19533 reportCircularBaseType (2310) | check.rs | MAIN |  |
| 98 | `BINDER-DUPLICATE-DEFAULT-EXPORT-SYMBOL` | 5 | 5 / 5 / 6 | 0 / 0 | binder/binder.go:152 declareSymbolEx (conflict arm: duplicate `default` gets a fresh unmerged symbol) | crates/tsr-binder/src/binder.rs (declare_symbol conflict for default exports) | MAIN |  |
| 99 | `GENERATOR-IMPLICIT-ANY-YIELD` | 5 | 0 / 0 / 0 | 5 / 5 | checker/checker.go:10952 checkYieldExpression TS7057 (+ reportImplicitAny:18275 TS7055/TS7025) | implicit_any.rs | MAIN |  |
| 100 | `ENUM-MEMBER-COMPUTED-NAME` | 5 | 0 / 0 / 0 | 5 / 5 | checker/checker.go:5121 checkEnumMember / grammarchecks computed enum member name (TS1164) without checking the expression | enum_member_name.rs | UNOWNED | **yes** |
| 101 | `SYMBOL-WELL-KNOWN-MEMBERS` | 5 | 0 / 0 / 0 | 5 / 5 | checker/checker.go well-known symbol property names on class/object literal members (Symbol.hasInstance assignment 2322, duplicate [Symbol.iterator] … | unique_symbol_keys.rs | UNOWNED | **yes** |
| 102 | `GENERIC-ARG-NIL-CONTEXTUAL-SIGNATURE` | 4 | 4 / 6 / 52 | 0 / 0 | checker/checker.go:10305 getContextualCallSignature (nil unless exactly one applicable sig) / :10264 getContextualSignature | signatures.rs:6214 get_type_of_function_expression (ContextualSignature::Absent => !singl… | r6-lazytext |  |
| 103 | `RETURN-LITERAL-OWN-SIG-CONTEXT` | 4 | 4 / 7 / 46 | 0 / 0 | checker/checker.go:20209 getReturnTypeFromBody (contextual return is a type variable -> literal kept via isLiteralOfContextualType) | signatures.rs::return_type_from_body | MAIN |  |
| 104 | `CREATE-UNION-OR-INTERSECTION-PROPERTY` | 4 | 4 / 4 / 42 | 0 / 0 | checker/checker.go:21452 createUnionOrIntersectionProperty (writeType union arm; optional/missing arm) | members.rs (union/intersection property synthesis) | MAIN |  |
| 105 | `NARROW-TYPE-BY-IN-KEYWORD` | 4 | 1 / 4 / 32 | 3 / 3 | checker/flow.go:1001 narrowTypeByInKeyword | flow.rs InKeyword arm | MAIN |  |
| 106 | `FUNCEXPR-NIL-CONTEXTUAL-SIGNATURE-POSITIONS` | 4 | 4 / 7 / 33 | 0 / 0 | checker/checker.go:29358 getContextualType (KindArrowFunction\\|KindReturnStatement arm) -> :29665 getContextualReturnType; :10264 getContextualSigna… | signatures.rs:4427 has_no_contextual_type (default arm :4644 returns false; ReturnStateme… | r6-lazytext |  |
| 107 | `RETURN-TYPE-FROM-BODY-WIDEN-UNIQUE-SYMBOL` | 4 | 4 / 4 / 30 | 0 / 0 | checker/checker.go:20126 getReturnTypeFromBody -> :20407 getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded -> :25515 getWidenedLiteralLikeType… | signatures.rs return_type_of_worker (no unique-symbol widening; literals.rs:119 get_widen… | MAIN |  |
| 108 | `FUNCEXPR-NIL-CONTEXTUAL-SIGNATURE-POSITIONS/ARROW-EXPRESSION-BODY` | 4 | 4 / 4 / 28 | 0 / 0 | checker/checker.go:29358 getContextualType (ArrowFunction body arm -> :29665 getContextualReturnType nil) -> assignNonContextualParameterTypes | signatures.rs::get_type_of_function_expression (context-sensitive gate) | MAIN |  |
| 109 | `QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE/ARGLESS-GENERIC` | 4 | 4 / 4 / 19 | 0 / 0 | checker/checker.go:23173-23198 getTypeFromClassOrInterfaceReference arity/errorType, :23206 fillMissingTypeArguments (:21954); :23595-23603 getTypeFr… | declared.rs:5086 argless arm (no local_type_parameters_of check) -> :5157 mint | MAIN |  |
| 110 | `FUNCEXPR-NIL-CONTEXTUAL-SIGNATURE-POSITIONS/PARAMETER-INITIALIZER` | 4 | 4 / 4 / 18 | 0 / 0 | checker/checker.go:29358 getContextualType (parameter-initializer arm -> nil) -> assignNonContextualParameterTypes | signatures.rs::get_type_of_function_expression gate | r6-lazytext |  |
| 111 | `SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER` | 4 | 4 / 6 / 13 | 0 / 0 | checker/symbolaccessibility.go:117 getWithAlternativeContainers / :342 getAliasForSymbolInContainer | checker.rs symbol_chain | MAIN |  |
| 112 | `UNION-PROPERTY-CONSTITUENT-THIS-INSTANTIATION` | 4 | 4 / 4 / 13 | 0 / 0 | checker/checker.go:21467 createUnionOrIntersectionProperty (per-constituent getPropertyOfType with members resolved per instantiation/thisArgument) | unions.rs / members.rs union property | MAIN |  |
| 113 | `EXPORT-ASSIGNMENT-ALIAS-LIKE-EXPRESSION` | 4 | 3 / 3 / 11 | 1 / 1 | checker/checker.go getTargetOfAliasLikeExpression (from getTargetOfExportAssignment) | symbols.rs alias-target expression arm | MAIN |  |
| 114 | `SIGNATURE-DECLARATION-ANNOTATION-REUSE/JSDOC` | 4 | 4 / 5 / 11 | 0 / 0 | checker/nodebuilderimpl.go:2181 serializeTypeForDeclaration (JSDoc annotation reuse) | signatures.rs::signature_to_string_at | r6-jsdoc |  |
| 115 | `CHECK-NEW-TARGET-META-PROPERTY` | 4 | 4 / 4 / 9 | 0 / 0 | checker/checker.go:10768 checkNewTargetMetaProperty (via :10753 checkMetaProperty) | check.rs:2149 check_new_target_meta_property (diagnostic only); objects.rs:907 check_obje… | MAIN |  |
| 116 | `ELEMENT-ACCESS-IMPLICIT-ANY-7053` | 4 | 0 / 0 / 0 | 4 / 8 | checker/checker.go:27001 getPropertyTypeForIndexType noImplicitAny arm (7053/7015/7017) | index_access_reports.rs | r6-relater2 |  |
| 117 | `ARRAY-LITERAL-ERROR-ELEMENT` | 4 | 4 / 4 / 8 | 0 / 0 | checker/checker.go:8021 checkArrayLiteral (8096 getUnionTypeEx, 8100 createArrayLiteralType(createArrayTypeEx)) | array_literals.rs:1144 check_array_literal_value (plain path `if element_type == error { … | MAIN |  |
| 118 | `CHECK-INDEXED-ACCESS-INDEX-TYPE` | 4 | 0 / 0 / 0 | 4 / 6 | checker/checker.go:8220 checkIndexedAccessIndexType (2536 arm 8253, 4105 arm 8248) | unported (index_access_reports.rs has no TS2536/TS4105 reporter; held in r5-relater6 §2.3) | r6-relater2 |  |
| 119 | `JSX-CONTEXTUAL-TYPE-NIL-ARMS` | 4 | 3 / 3 / 5 | 1 / 1 | checker/jsx.go:214 getContextualTypeForJsxAttribute / :241 getContextualTypeForChildJsxExpression | jsx_intrinsic.rs:723 jsx_attribute_context / :770 jsx_child_context | MAIN |  |
| 120 | `GET-NON-UNDEFINED-TYPE-GENERIC-CONSTRAINT` | 4 | 2 / 2 / 4 | 2 / 2 | checker/checker.go:31548 getNonUndefinedType (generic with undefined constraint -> base constraint) | destructure.rs | MAIN |  |
| 121 | `ASSIGN-REPORT-ELABORATION-CHOICE` | 4 | 0 / 0 / 0 | 4 / 6 | checker/relater.go reportRelationError / elaborateError (2322 vs 2739/2740/2741/2353 head selection, object literal excess property) | assignreport.rs | r6-relater2 |  |
| 122 | `INDEXED-ACCESS-TYPE-NODE-CHECKS` | 4 | 0 / 0 / 0 | 4 / 6 | checker/checker.go:8220 checkIndexedAccessIndexType / :8133 checkIndexedAccess (2536/2537/2538) + checker.go:3385 checkMappedType | indexed.rs | r6-errorsplit2 |  |
| 123 | `QUALIFIED-NAME-LEFT-ALIAS-RESOLVE` | 4 | 4 / 4 / 6 | 0 / 0 | checker/checker.go:15828 resolveQualifiedName | symbols.rs::resolve_qualified_entity | MAIN |  |
| 124 | `GET-THIS-TYPE-ERRORTYPE-COMPOSES` | 4 | 4 / 4 / 6 | 0 / 0 | checker/checker.go:22908 getThisType (errorType at :22919) | declared.rs:3847 get_type_from_this_type_node | r6-declared2 |  |
| 125 | `ENUM-RELATION-SAME-NAME` | 4 | 1 / 1 / 2 | 3 / 3 | checker/relater.go:enumRelatedTo / isEnumTypeRelatedTo (same-named enums across declarations, computed members) | relater.rs | r6-relater2 |  |
| 126 | `NODEBUILDER-TRACKER-DIAGNOSTICS` | 4 | 0 / 0 / 0 | 4 / 4 | checker/nodebuilderimpl.go (cyclic structure 5088, truncation 7056) + transformers/declarations/tracker.go:112 ReportNonSerializableProperty (4118) | unported | UNOWNED | **yes** |
| 127 | `REPORT-RELATION-ERROR-UNMATCHED-PROPERTY` | 4 | 0 / 0 / 0 | 4 / 4 | checker/relater.go:4345 reportUnmatchedProperty (via structuredTypeRelatedTo -> propertiesRelatedTo, 2741/2739/2740) | assignreport.rs | r6-relater2 |  |
| 128 | `CHECK-RESOLVED-BLOCK-SCOPED-VARIABLE` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:1888 checkResolvedBlockScopedVariable (2448/2449/2450, enum arms under isolatedModules) | check.rs | MAIN |  |
| 129 | `IMPORT-ALIAS-MERGED-WITH-LOCAL-DECLARATION` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:6736 checkAliasSymbol (2440) / 6955 checkExportsOnMergedDeclarations (2395) on an alias merged with a local value | symbols.rs | MAIN |  |
| 130 | `CATCH-CLAUSE-VARIABLE-ANNOTATION-ERRORTYPE` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:checkVariableLikeDeclaration / getTypeForVariableLikeDeclaration catch-clause arm (unknown/any annotation, useUnknownInCatchVariab… | symbols.rs | MAIN |  |
| 131 | `GENERIC-CALL-DECLINED-ERRORTYPE` | 4 | 0 / 0 / 0 | 4 / 4 | checker/inference.go inferTypes/getInferredType + checker.go resolveCall | calls.rs check_generic_call_worker | MAIN |  |
| 132 | `CHECK-MODULE-AUGMENTATION-ELEMENT` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:5238 checkModuleAugmentationElement (2666/2667) | unported | MAIN |  |
| 133 | `CHECK-GLOBAL-AUGMENTATION-GRAMMAR` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:5130 checkModuleDeclaration global-augmentation arms (2669/2670) | check.rs (2669 only) | MAIN |  |
| 134 | `JS-REQUIRE-CALL-MODULE-RESOLUTION` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:15101 resolveExternalModuleName (checkCallExpression require-call arm, isRequireCall) | calls.rs / symbols.rs | r6-modules2 |  |
| 135 | `HAS-EXCESS-PROPERTIES-TARGET-ARMS` | 4 | 0 / 0 / 0 | 4 / 4 | checker/relater.go:2714 hasExcessProperties (isKnownProperty / isExcessPropertyCheckTarget) | assignreport.rs (excess-property path) | r6-relater2 |  |
| 136 | `REVERSE-MAPPED-TYPE-INFERENCE` | 4 | 0 / 0 / 0 | 4 / 4 | checker/inference.go:1066 inferReverseMappedType (+ inferTypeForHomomorphicMappedType constraint intersection) | inference.rs | MAIN |  |
| 137 | `REPORT-UNMATCHED-PROPERTY-HEAD` | 4 | 0 / 0 / 0 | 4 / 4 | checker/relater.go:4345 reportUnmatchedProperty (reportRelationError:4751 missing-properties arm) | assignreport.rs | r6-relater2 |  |
| 138 | `RESOLVE-EXTERNAL-MODULE-EXTENSION-ARMS` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:15149 resolveExternalModule (TS2876 rewrite-unsafe, TS5097 .ts extension, TS2846 .d.ts import) | symbols.rs::resolve_external_module_name | r6-modules2 |  |
| 139 | `TYPE-ONLY-EXPORT-CHAIN-RESOLUTION` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go resolveAlias / getExportsOfModule through `export type` and `export * as ns` chains | symbols.rs | r6-modules2 |  |
| 140 | `PARSER-STATIC-BLOCK-AWAIT-CONTEXT` | 4 | 0 / 0 / 0 | 4 / 4 | parser/parser.go parseClassStaticBlockDeclaration (await context) + createIdentifierWithDiagnostic:5873 | tsr-parser/src | MAIN |  |
| 141 | `WITH-STATEMENT-BODY-UNCHECKED` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:4156 checkWithStatement (body is not type-checked) | strict_mode.rs::check_with_statement | MAIN |  |
| 142 | `GENERATOR-RETURN-TYPE-ANNOTATION-CHECK` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go:3461-area checkFunctionLikeDeclaration -> checkSignatureDeclaration generator return type (createGeneratorType assignable to annot… | unported (check.rs function-like declaration check) | MAIN |  |
| 143 | `GRAMMAR-TYPE-PARAMETER-MODIFIERS` | 4 | 0 / 0 / 0 | 4 / 4 | checker/grammarchecks.go:214 checkGrammarModifiers (type-parameter arms: const/in/out/accessibility) + checker.go checkVarianceAnnotations (2636/2637) | unported (grammar.rs / variances.rs) | MAIN |  |
| 144 | `RELATER-GENERIC-SOURCE-FALSE-ASSIGNABLE` | 4 | 0 / 0 / 0 | 4 / 4 | checker/relater.go structuredTypeRelatedTo (type-parameter/union-of-type-parameters source to {} / mapped-with-as-clause / indexed-access target) | relater.rs | r6-relater2 |  |
| 145 | `UNION-SIGNATURE-CALL-CONSTRUCT` | 4 | 0 / 0 / 0 | 4 / 4 | checker/checker.go resolveCallExpression/resolveNewExpression on union types (getUnionSignatures; abstract construct 2511; this-arg vs 2741) | union_signatures.rs | r6-lazytext |  |
| 146 | `GRAMMAR-BINDING-PATTERN-REST` | 4 | 0 / 0 / 0 | 4 / 4 | checker/grammarchecks.go:671 checkGrammarForDisallowedTrailingComma + :1536 checkGrammarBindingElement (rest initializer 1186, rest binding pattern 2… | unported (grammar.rs / destructure.rs) | MAIN |  |
| 147 | `PARSE-UNARY-OPERAND-LESS-THAN-RECOVERY` | 4 | 4 / 4 / 4 | 0 / 0 | parser/parser.go:4668 parseUnaryExpressionOrHigher (+ :5132 parseTypeAssertion, :5090 parsePrefixUnaryExpression) | crates/tsr-parser (unary/type-assertion parse) | MAIN |  |
| 148 | `IS-ONLY-IMPORTABLE-AS-DEFAULT-DJSON-ARM` | 4 | 4 / 4 / 4 | 0 / 0 | checker/checker.go:14800 isOnlyImportableAsDefault (`.d.json.ts` declaration-file-extension arm :14812) | symbols.rs:1849 is_only_importable_as_default (JSON_FILE flag only) | MAIN |  |
| 149 | `TYPEOF-SELF-REFERENCE-CIRCULARITY` | 3 | 3 / 3 / 73 | 0 / 0 | checker/checker.go:24102 getTypeFromTypeQueryNode -> getTypeOfSymbol circularity (pushTypeResolution) => any | symbols.rs variable type resolution / declared.rs type-query arm (deferred `typeof x`) | MAIN |  |
| 150 | `HIGHER-ORDER-GENERIC-ARG-INSTANTIATION` | 3 | 3 / 3 / 47 | 0 / 0 | checker/checker.go:7599 instantiateTypeWithSingleGenericCallSignature | inference.rs (argument check of a generic function value; propagate_return_type_parameter… | MAIN |  |
| 151 | `INTERFACE-BASE-FROM-TYPE-NODE` | 3 | 3 / 4 / 47 | 0 / 0 | checker/checker.go:19498 resolveBaseTypesOfInterface (+ :19537 isValidBaseType) | members.rs::base_symbols_of_ex (base_symbol_of_heritage_entry) | MAIN |  |
| 152 | `ARRAY-LITERAL-SUBTYPE-REDUCTION` | 3 | 3 / 4 / 38 | 0 / 0 | checker/checker.go:8021 checkArrayLiteral (getUnionTypeEx(elementTypes, UnionReductionSubtype)) | array_literals.rs (element union literal reduction only) | MAIN |  |
| 153 | `KEYOF-RESOLVED-OPERAND-INDEX-TYPE` | 3 | 3 / 6 / 35 | 0 / 0 | checker/checker.go:26680 getIndexType (from :22960 getTypeFromTypeOperatorNode) | declared.rs::get_type_from_type_node keyof arm | r6-declared2 |  |
| 154 | `MODULE-AUGMENTATION-MERGE/REEXPORT-DECLINE-GATE` | 3 | 3 / 3 / 26 | 0 / 0 | checker/checker.go:15852 resolveQualifiedName getMergedSymbol(getSymbol(getExportsOfSymbol(ns))) | declared.rs:5467 alias_rooted_reference_declines (augmentation arm :5476-5501; doc :5457)… | MAIN |  |
| 155 | `CTOR-ASSIGNED-PROPERTY-REFERENCE-FORMS` | 3 | 3 / 3 / 25 | 0 / 0 | checker/flow.go:2466 getFlowTypeInConstructor (references this['x'], this[0], this.#x) | flow.rs reference matching / class property type from constructor assignments | MAIN |  |
| 156 | `GLOBALTHIS-SYMBOL-IN-GLOBALS/INTERSECTION-MEMBERS` | 3 | 3 / 3 / 24 | 0 / 0 | checker/checker.go:21467 createUnionOrIntersectionProperty over typeof globalThis (members from resolveAnonymousTypeMembers of globalThisSymbol) | members.rs (intersection property lookup; globalThis special-cased only for `globalThis.x… | MAIN |  |
| 157 | `NARROWABLE-REF-BINDING-PATTERN-CONTEXT` | 3 | 3 / 4 / 21 | 0 / 0 | checker/checker.go:31491 getNarrowableTypeForReference / :31533 hasContextualTypeWithNoGenericTypes | constraints.rs narrowable_type_for_reference | MAIN |  |
| 158 | `CLASS-INSTANCE-MEMBERS-FROM-BASE-TYPES/INHERITED-THIS-ARGUMENT` | 3 | 3 / 3 / 15 | 0 / 0 | checker/checker.go:19106 resolveObjectTypeMembers (inherited members instantiated with thisArgument) | members.rs | MAIN |  |
| 159 | `NARROWABLE-REF-CONTEXTUAL` | 3 | 3 / 3 / 14 | 0 / 0 | checker/checker.go getNarrowableTypeForReference (substituteConstraints in constraint position / no-generic contextual type) | constraints.rs::narrowable_type_for_reference | MAIN |  |
| 160 | `INFER-TYPE-PREDICATE-FROM-BODY` | 3 | 2 / 2 / 12 | 1 / 1 | checker/checker.go:20535 getTypePredicateFromBody | signatures.rs::infer_type_predicate_from_body | r6-lazytext |  |
| 161 | `DESTRUCTURE-FLOW-DEPENDENT` | 3 | 1 / 2 / 10 | 2 / 2 | checker/checker.go getFlowTypeOfDestructuring (from :17707 getBindingElementTypeFromParentType) | destructure.rs | MAIN |  |
| 162 | `EXISTING-NODE-TRACK-ENTITY-NAME` | 3 | 3 / 3 / 12 | 0 / 0 | checker/nodecopy.go:317 trackExistingEntityName (in :288 getExistingNodeTreeVisitor, used by serializeTypeForDeclaration nodebuilderimpl.go:2181) | signatures.rs:5010 written_annotation_text | UNOWNED | **yes** |
| 163 | `MAPPED-CONDITIONAL-WRITTEN-TEXT-MINT` | 3 | 3 / 4 / 10 | 0 / 0 | checker/checker.go getTypeFromMappedTypeNode (semantic mapped type in a predicate) | declared.rs (mapped node minted as print-only text) | r6-declared2 |  |
| 164 | `GET-CONTAINERS-OF-SYMBOL-ALTERNATIVE-CONTAINING-MODULES` | 3 | 3 / 3 / 9 | 0 / 0 | checker/symbolaccessibility.go:168 getAlternativeContainingModules (via getContainersOfSymbol) | symbol_accessibility.rs (container walk) | MAIN |  |
| 165 | `GET-PROPERTY-OF-TYPE-INCLUDE-TYPE-ONLY-MEMBERS` | 3 | 3 / 3 / 8 | 0 / 0 | checker/checker.go:18899 getPropertyOfTypeEx(includeTypeOnlyMembers) (caller :11323) | members.rs::get_property_of_type_ex | MAIN |  |
| 166 | `CONTEXTUAL-TYPE-FOR-ASSIGNMENT-EXPRESSION-JS` | 3 | 3 / 3 / 8 | 0 / 0 | checker/checker.go:29843 getContextualTypeForAssignmentExpression | signatures.rs::has_no_contextual_type (assignment arm) | r6-lazytext |  |
| 167 | `CONTEXTUAL-ARG-GENERIC-MAPPED` | 3 | 1 / 1 / 4 | 2 / 2 | checker/checker.go:getContextualTypeForObjectLiteralElement over generic mapped/intersection contextual type | contextual.rs | MAIN |  |
| 168 | `RECURSION-DEPTH` | 3 | 1 / 3 / 4 | 2 / 2 | checker/relater.go:isDeeplyNestedType / recursion identity | relater.rs | r6-declared2 |  |
| 169 | `RESOLVE-DECORATOR-SCOPE` | 3 | 3 / 3 / 6 | 0 / 0 | checker/binder/checker resolveName for decorated class references | resolution.rs | r6-names2 |  |
| 170 | `SHADOWED-TYPEPARAM-RENAME/FREE-PARAM-BYTEXT` | 3 | 3 / 3 / 6 | 0 / 0 | checker/nodebuilderimpl.go:1404 typeParameterToName (typeParameterNamesByText / typeParameterNames cache) | inference.rs:5767 rename_type_parameters_for_site (:5834 byText refused); printing.rs:150… | r6-lazytext |  |
| 171 | `AWAITED-THIS-TYPE` | 3 | 3 / 3 / 6 | 0 / 0 | checker/checker.go getAwaitedType of a this-type (async arrow `await this`) | iteration.rs/awaited | UNOWNED | **yes** |
| 172 | `INFERENCE-MATCHING-INTERSECTION-CONSTITUENTS` | 3 | 3 / 3 / 6 | 0 / 0 | checker/inference.go:65 inferFromTypes (intersection arm) -> :370 inferFromMatchingTypes | inference.rs infer_from_types (intersection target) | MAIN |  |
| 173 | `BINDING-PATTERN-IMPLIED-TYPE/EXPRESSION-PARAM-ANY-DECLINE` | 3 | 3 / 3 / 6 | 0 / 0 | checker/checker.go:16748 getTypeForVariableLikeDeclaration (initializer arm) | signatures.rs::parameter_of | r6-lazytext |  |
| 174 | `INFER-REVERSE-MAPPED-TYPE` | 3 | 2 / 2 / 4 | 1 / 1 | checker/inference.go:948 inferToMappedType -> :1066 inferReverseMappedType | inference.rs reverse mapped inference | MAIN |  |
| 175 | `RELATE-CONDITIONAL` | 3 | 0 / 0 / 0 | 3 / 4 | checker/relater.go:structuredTypeRelatedToWorker conditional-type arms (source/target ConditionalType, Extract/deferred) | relater.rs | r6-relater2 |  |
| 176 | `CHECK-GRAMMAR-INDEX-SIGNATURE-PARAMETERS` | 3 | 0 / 0 / 0 | 3 / 4 | checker/grammarchecks.go:796 checkGrammarIndexSignatureParameters (1021/1025/1096/1268/1337) | check.rs index signature grammar | MAIN |  |
| 177 | `PARSE-TYPE-ARGUMENTS-IN-EXPRESSION-EMPTY-LIST` | 3 | 1 / 1 / 2 | 2 / 2 | parser/parser.go:parseTypeArgumentsInExpression (empty <>) | crates/tsr-parser | MAIN |  |
| 178 | `CHECK-TYPE-ARGUMENT-CONSTRAINTS-GENERIC` | 3 | 0 / 0 / 0 | 3 / 4 | checker/checker.go:3016 checkTypeArgumentConstraints | constraints.rs::check_type_argument_constraints | MAIN |  |
| 179 | `CHECK-NON-NULL-TYPE-UNKNOWN-REPORTER` | 3 | 0 / 0 / 0 | 3 / 4 | checker/checker.go:7413 checkNonNullTypeWithReporter (unknown arm TS18046) | unknown_operand.rs (ported, held) | MAIN |  |
| 180 | `REMOVE-SUBTYPES-COMPLEXITY-LIMIT` | 3 | 1 / 1 / 2 | 2 / 2 | checker/checker.go:25934 removeSubtypes / 26648 checkCrossProductUnion (TS2590) | unported | MAIN |  |
| 181 | `TYPE-ALIAS-ACCESSIBILITY-GATE/DUPLICATE-DECLARATION` | 3 | 3 / 3 / 4 | 0 / 0 | checker/checker.go getDeclaredTypeOfTypeAlias / alias symbol for duplicate declarations | declared.rs:3961 alias_symbol_for_type_node | MAIN |  |
| 182 | `CIRCULARITY-IMPLICIT-ANY-REPORTS` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:18837 reportCircularityError / getReturnTypeOfSignature 20001 circularity (7022/7023/7024) | symbols.rs, signatures.rs | MAIN |  |
| 183 | `GRAMMAR-EMPTY-TYPE-PARAMETER-OR-ARGUMENT-LIST` | 3 | 0 / 0 / 0 | 3 / 3 | checker/grammarchecks.go:678 checkGrammarTypeParameterList (1098) / 845 checkGrammarForAtLeastOneTypeArgument (1099), checkGrammarTypeArguments trail… | unported | MAIN |  |
| 184 | `RESOLVE-ALIAS-CIRCULARITY-REPORT` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:16266 resolveAlias (circular 2303) + resolveEntityName for import-equals target (2304) | circular_alias.rs | MAIN |  |
| 185 | `REPORT-RELATION-ERROR-HEAD-MESSAGE-ARMS` | 3 | 0 / 0 / 0 | 3 / 3 | checker/relater.go:4751 reportRelationError (2820 suggested literal, 2719 same-named types) | assignreport.rs | r6-relater2 |  |
| 186 | `RELATE-VARIANCES-UNRELIABLE-STRUCTURAL-FALLBACK` | 3 | 0 / 0 / 0 | 3 / 3 | checker/relater.go:relateVariances / structural fallback when variance result is unreliable | variances.rs | r6-relater2 |  |
| 187 | `IMPLICIT-ANY-SETTER-ONLY-ACCESSOR` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:18511 getTypeOfAccessors (7032) + parameter 7006 for the setter | unported (7032 has no port use) | MAIN |  |
| 188 | `CHECK-MODULE-ELEMENT-CONTEXT-GRAMMAR` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:5266 checkImportDeclaration / 5503 checkExportDeclaration / 5130 checkModuleDeclaration (1232/1233/1234/1473) | unported | MAIN |  |
| 189 | `CHECK-INDEX-CONSTRAINTS` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:4786 checkIndexConstraints (2411/2413) | index_constraint.rs | UNOWNED | **yes** |
| 190 | `MAPPED-TYPE-AS-CLAUSE-MEMBERS` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:20894 resolveMappedTypeMembers / 26680 getIndexType (as-clause key remapping) | mapped.rs | r6-declared2 |  |
| 191 | `ACCESSOR-CIRCULAR-ANY` | 3 | 2 / 2 / 2 | 1 / 1 | checker/checker.go getTypeOfAccessors (circularity -> any) | symbols.rs:334 get_type_of_accessors_worker (None -> error :408) | MAIN |  |
| 192 | `UMD-GLOBAL-MERGE` | 3 | 0 / 0 / 0 | 3 / 3 | binder/binder.go mergeSymbolTable for UMD `export as namespace` globals | tsr-binder | MAIN |  |
| 193 | `CHECK-UNUSED-RENAMED-BINDING-ELEMENTS` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:7314 checkUnusedRenamedBindingElements | unported | MAIN |  |
| 194 | `PROGRAM-VERIFY-COMPILER-OPTIONS` | 3 | 0 / 0 / 0 | 3 / 3 | compiler/program.go:1005 verifyCompilerOptions (TS5090 paths, TS5095/TS5109 moduleResolution) | tsr-compiler | r6-modules2 |  |
| 195 | `PARSER-RESERVED-WORD-BINDING-RECOVERY` | 3 | 0 / 0 / 0 | 3 / 3 | parser/parser.go:3311 parseParameter / parseBindingIdentifier:5810 / import-equals name | tsr-parser/src | MAIN |  |
| 196 | `CHECK-CLASS-LIKE-HERITAGE-ARMS` | 3 | 0 / 0 / 0 | 3 / 3 | checker/checker.go:4293 checkClassLikeDeclaration (TS2422/TS2500/TS2510) + resolveBaseTypesOfClass:19244 TS2508 | unported arms | MAIN |  |
| 197 | `GRAMMAR-USING-DECLARATION` | 3 | 0 / 0 / 0 | 3 / 3 | checker/grammarchecks.go:1646 checkGrammarVariableDeclarationList / :1200 checkGrammarForInOrForOfStatement (using arms) | unported (grammar.rs) | MAIN |  |
| 198 | `PARSE-TYPE-ARGUMENTS-IN-EXPRESSION` | 3 | 3 / 3 / 3 | 0 / 0 | parser/parser.go parseTypeArgumentsInExpression / parseCallExpressionRest / parseNewExpressionOrNewDotTarget | crates/tsr-parser (expression type-argument lookahead) | MAIN |  |

The full per-cluster records (all 759, with case lists) are reproducible
from the dumps by the method above. Every cluster unlocking ≥5 cases is a
proposed issue in `r6-triage-issues.json` (101 entries, plus the two
remainders of §4).

## 3. Reading the table

**Two MAIN decline families lead the diagnostics side.** Neither is a
missing operation: each is a port-side refusal with no native counterpart.
- `PARSE-ERROR-FILE-CHECK-DECLINE` (43 cases): about 170
  `if self.file_has_parse_errors { return }` sites in `check.rs`,
  `calls.rs`, `nonexistent_property.rs` and others withhold `c.error`
  reports. Native withholds only `grammarErrorOnNode` reports in such a file
  (`hasParseDiagnostics`). Sampled by removing the parse error from the
  source: the port then reports the native TS2345/TS2378. Removing the gates
  wholesale will surface cascades wherever the port's parser recovery
  differs (`PARSER-RECOVERY-DIVERGENCE`), so lift them per site, measured.
- `NONEXISTENT-PROPERTY-CERTIFICATION-GATE` (24): `nonexistent_property.rs`
  refuses TS2339 on a union or narrowed receiver once a condition or call
  flow node precedes the access. Repro: an unrelated `f(1)` before `x.a`
  with `x: {a:1}|{b:1}` silences the report. The types already match native
  there.
- `JS-FILE-CHECK-DECLINE` (17) is the same pattern for `in_js_file`.

**Call resolution reporting** (`calls.rs`, MAIN) is the next block. It covers
`CALL-ARGUMENT-APPLICABILITY-REPORT` (36), `CANDIDATE-FOR-OVERLOAD-FAILURE`
(22), `CALL-SPREAD-ARGUMENT-APPLICABILITY` (16), `OVERLOAD-FAILURE-REPORT`
(15) and `TAGGED-TEMPLATE-EFFECTIVE-ARGS` (17): 106 cases behind
resolveCall's reporting pass, `getSignatureApplicabilityError` with
`reportErrors`, and the tagged-template effective arguments.

**Families to split before dispatch.** These rows were grouped by area:
- `PRIVATE-IDENTIFIER-GRAMMAR` (17, unowned) is about twelve operations:
  - TS18030, the optional chain: privateIdentifierChain.1,
    privateNameUncheckedJsOptionalChain;
  - TS18012, `#constructor`: privateNameConstructorReserved,
    plainJSBinderErrors;
  - TS18011, delete: privateNamesNoDelete;
  - TS2804, static/instance: privateNamesUnique-3;
  - TS18019/TS1024, modifiers: privateNamesIncompatibleModifiers;
  - TS18014, shadowing: privateNameNestedMethodAccess;
  - TS18046 on `unknown`: privateNameAndAny;
  - TS2339 through a rest: privateNameAndObjectRestSpread;
  - `#x in` checks: privateNameInInExpression ×2;
  - TS7022 circularity: privateNameCircularReference;
  - TS18016 outside a class: privateNameInObjectLiteral-3,
    plainJSGrammarErrors4;
  - privateNameImplicitDeclaration.

  No single arm converts more than two cases.
- `PARSER-RECOVERY-DIVERGENCE`, `GRAMMAR-MISC-DECLARATIONS`,
  `CHECK-GRAMMAR-MODIFIERS`, `SCANNER-NUMERIC-AND-ESCAPE-DIAGNOSTICS`,
  `JSDOC-TAG-SEMANTICS`, `MODULE-RESOLUTION-DIAGNOSTICS`,
  `CLASS-PROPERTY-INITIALIZER-CHECKS` and
  `CHECK-INTERFACE-HERITAGE-AND-BASES` are each several single-site arms in
  one owner's file. Their counts are real, but the owner works them arm by
  arm.

**Types side.** `TYPE-ALIAS-DECLARED-BODY-AND-INSTANTIATION` (34 cases, 323
lines) is still the largest root. It belongs to r6-declared2, together with
`CONDITIONAL-INLINE-NODE-INSTANTIATION` (10 cases, 167 lines: a conditional
written inline, not through an alias, instantiates to the gap, e.g.
`P<string>["p"]` with `type P<T> = {p: T extends string ? 1 : 2}`),
`TYPE-ALIAS-INSTANTIATION-NEW-ALIAS` (11) and
`TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL` (10). The symbol-chain printer
(`.39`, MAIN) owns `EXPORT-SPECIFIER-NOT-IN-SCOPE` (23),
`SYMBOL-CHAIN-LOCAL-IMPORT-EQUALS-ALIAS` (19), `GET-SYMBOL-CHAIN-NEEDS-QUALIFICATION`
(16), `TRY-SYMBOL-TABLE-DEFAULT-IMPORT-ALIAS` (11) and
`SYMBOL-CHAIN-CANDIDATE-EXPORTS-OF-SYMBOL` (10): 79 cases.

**Broad value-side defects found while reducing repros.** Each is cheap to
reproduce and likely reaches past its packet. Owners in parentheses.
- `export { I } from "./a"` re-exported, then `import { I } from "./b"; let
  i: I` types `i` as the gap. The `export *` form works.
  `RESOLVE-ALIAS-INDIRECTION`, 18 cases (`symbols.rs`, MAIN).
- `export default class C {}` followed by a value reference to `C` in the
  same file is the gap (`IDENTIFIER-EXPORT-SYMBOL-OF-VALUE`, MAIN).
- Any type use of a renamed import (`import { X as Y }`) is the gap. This is
  a deliberate decline in `declared.rs`, the name-agreement gate
  (`RESOLVE-TYPE-REFERENCE-NAME-RENAMED-IMPORT`, r6-declared2).
- `new Z(x)` fails when Z's type parameters carry `in`/`out`
  (`VARIANCE-ANNOTATED-CLASS-CONSTRUCT`, no port location found).
- `this` inside a decorator is native `typeof globalThis`.
  `check_this_expression`'s own ancestor walk lacks getThisContainer's
  Decorator arm (`GET-THIS-CONTAINER-DECORATOR-ARM`, 5, unowned,
  `expressions.rs`).
- After a failed `x.nope()` on `"foo"|"bar"`, later TS2339 reports on unions
  containing `"foo"` are lost: state leaks across reports
  (`FAILED-METHOD-CALL-SUPPRESSES-LATER-PROPERTY-ERRORS`, MAIN).
- `nodeModulesCjsFormatFileAlwaysHasDefault` fails only when the case lists
  `package.json` after the `.ts` files. The CLI on real files is right, so
  this is the harness's package scope order (`HARNESS-PACKAGE-JSON-SCOPE-ORDER`).

**Held diffs still pay.** `SPREAD-PROPERTY-ANNOTATION-REUSE` (12 cases) is
the held `r6-nodereuse-property-slot-spreads.diff` (waits on `tsr-2zk.1120`).
`ORIGIN-SLICE-GATE` (5) is the held
`r6-typesroots-HELD-origin-slice-gate.diff`, which waits on type-literal
circular members.

## 4. Ported: `checkImportAttributes`' relation and TS2858

**Choice.** The top unowned row, `PRIVATE-IDENTIFIER-GRAMMAR`, is a family
whose largest single operation converts two cases (§3). The largest unowned
**single-operation** cluster is `IMPORT-ATTRIBUTES-CHECKS`' core. Two native
arms report on the same `with { … }` clause, and five of the cluster's nine
cases need both:
- TS2322, `checkImportAttributes` (`checker.go:5413-5416`): the attributes
  object related to `ImportAttributes | undefined`;
- TS2858, `checkExternalImportOrExportDeclaration`'s attribute loop
  (`checker.go:5361-5371`): a value that is not a string literal.

The other four cases are different operations, filed in
`r6-triage-issues.json`:
- `grammarErrors`: TS1363/TS1392 (`checkGrammarImportClause`);
- `importSpecifiers1`: TS2206;
- `importTag15` ×2: the JSDoc `@import` attributes, which the port's check
  walk does not reach (r6-jsdoc).

The next unowned single operation, `IMPORT-DEFER-CALL-EXPRESSION` (6 types
cases), needs its hook in `calls.rs` (MAIN), so either choice ships the same
way.

**Forcing constraint.** `import_attributes.rs` (r5-modules §3) ported every
grammar arm of `checkImportAttributes` but left out the opening relation.
`getTypeFromImportAttributes` (`checker.go:5444`) builds an anonymous
object-literal type whose symbol the checker mints
(`newSymbol(SymbolFlagsObjectLiteral, "__importAttributes")`). The port's
types name binder symbols only, and two relater facts read the symbol:
- `isObjectTypeWithInferableIndex` (`relater.go:4624`) admits
  `{ field: "a" }` against `ImportAttributes`' string index;
- the relater's `has_members` decides whether a structural comparison
  happens at all. Without members the relation answered `Unknown` for every
  pair.

TS2858 had no emitter.

**Port.** The new file `import_attribute_checks.rs` holds:
- `check_import_attribute_values`: the loop, behind
  `checkExternalImportOrExportDeclaration`'s earlier arms (string module
  name, position, the relative-name-in-ambient-module arm with
  `isTopLevelInExternalModuleAugmentation`). It returns native's `hasError`.
- `check_import_attributes_assignable`: the relation. A missing global
  `ImportAttributes` is native's `emptyObjectType`, so nothing is related.
  No expression is passed, so nothing is elaborated.
- `type_from_import_attributes`: the object. Its members are
  `getRegularTypeOfLiteralType(checkExpression(value))`, and the last
  duplicate wins. It is registered in `anonymous_properties`,
  `object_literal_members`, `object_literal_spread_flags` (ObjectLiteral, no
  spread) and `non_inferrable_types`.

**Hooks** (`r6-triage-import-attribute-values.diff`):
- `checker.rs` (MAIN): a new side table, `minted_object_literal_symbol_types`.
  It holds the anonymous types whose native symbol is a checker-minted
  ObjectLiteral symbol (ADR-0003 rather than widening `TypeData`). Written
  once at mint, never removed.
- `relater.rs` (r6-relater2): two reads, each placed after the existing arms
  have failed so the hot path pays nothing. `has_members` takes the table on
  a `Named { members: None }` type, and `object_type_has_inferable_index` on
  the symbol-less fallthrough.
- `index_signatures.rs` (unowned): `is_object_type_with_inferable_index`,
  the same fallthrough.
- `import_attributes.rs` (unowned): the relation runs before the
  parse-error gate, because it is a `c.error`, not a grammar report.
- `check.rs` (MAIN): `check_import_attribute_values` in the
  `ImportDeclaration` and `ExportDeclaration` arms.
- The two `#[expect(dead_code)]` allowances come out. The new file gains
  the two side-table writes.
- New tests: `crates/tsr-conformance/tests/import_attribute_checks.rs`
  (three cases, every expectation read off `tsgo`).

**Checker port convention.** The native operation is
`getTypeFromImportAttributes`, cached in `typeNodeLinks[attributes]`. The
port keeps no cache: the check walk visits each declaration once, and
nothing else asks for the type. The side table's key is the minted `TypeId`
and its owner the Checker. Entries are published at mint and are never
provisional. The work is one `check_expression` per attribute value and one
relation per declaration that has attributes.

**Not ported, accepted.** When TS2858 fires, native's
`checkExternalImportOrExportDeclaration` answers false and the caller skips
the import/export binding checks (`checker.go:5281`, `:5515`). `check.rs`
does not consume the returned bool. No corpus case shows a binding
diagnostic beside a TS2858, and the issue is filed. The CLI prints TS2322
without native's elaboration chain, as it does for every TS2322 today.

**Measured** (diff applied on `e6eadf4`, both dumps unfiltered):
- diagnostics: WRONG→RIGHT for `compiler/importAssertionNonstring`,
  `conformance/importAttributes6` (node18, node20, nodenext) and
  `conformance/importAttributes9`. That is **+5** (RIGHT 5,612 → 5,617).
- types: byte-identical (the `cut -f1-4` streams compare equal).
- both loss checks are empty; slowcases is clean on both dumps.
- Ir: domain-model 1,092,484,408 → 1,092,399,593 (−0.008%); generic-imports
  343,109,097 → 343,057,174 (−0.015%).

The first draft tested the table before the existing arms and read dm
+0.060%. Moving the reads to the fallthrough removed that cost.

**Falsifiers.**
- A relation that should hold but now reports: a type in the new table that
  some other arm decides differently would show as a new extra TS2322 in a
  dump.
- A TS2858 case whose binding diagnostics native skips: an extra binding
  diagnostic next to TS2858.

## 5. Diffs, in apply order

1. [`r6-triage-import-attribute-values.diff`](r6-triage-import-attribute-values.diff)
   (§4). It touches `checker.rs`, `check.rs` (MAIN), `relater.rs`
   (r6-relater2), `index_signatures.rs` and `import_attributes.rs`
   (unowned), `import_attribute_checks.rs` (this box), and adds tests. +5
   diagnostics, 0 lost, types identical.
