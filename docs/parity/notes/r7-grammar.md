# r7-grammar — lane notes

Round-7 lane `tsr-2zk.1271`: checker-side grammar. Pinned native source:
`vendor/typescript-go` @ `5b1047d`. Owned files: `grammar.rs`,
`import_attributes.rs`, `class_fields.rs`, `enum_member_name.rs`,
`enum_initializer.rs`, `unused.rs`, `heritage_conformance.rs`,
`base_types.rs`. Frozen base: `9020aa67`.

## §1 `checkGrammarModifiers` as one function

**Forcing fact.** r6-triage's `CHECK-GRAMMAR-MODIFIERS` row (14 diagnostics
cases finished alone) and the codes it names had no emitter or a wrong one:
TS1029/TS1243 arms missing for `abstract` and `accessor`, TS1275, TS1276,
TS1207, TS1495, TS1079, TS8038. The cause was structural, not a missing arm.
The port had grown `checkGrammarModifiers` as ten functions in `check.rs`
(`check_modifier_order`, `check_index_signature_modifiers`,
`check_type_parameter_modifier`, `check_illegal_decorator`,
`check_decorated_private_name`, `check_abstract_modifier_position`,
`check_grammar_async_modifier`, `check_modifier_on_nested_statement`,
`check_grammar_default_and_const_modifiers` and the parameter-property and
`abstract` arms of `check_grammar_modifier_shapes`), plus
`check_grammar_decorator_target` in `grammar.rs`. Each restated some of the
others' exclusions to approximate upstream's single `return` (§857, §871, §876
and §878 of the old notes record four such repairs). The approximation still
lost the `return`:

- `functionsWithModifiersInBlocks1`: TS1184 (`reportObviousModifierErrors`)
  and then TS1029 from the order walk on the same node; upstream returns at
  the first.
- `esDecorators-classDeclaration-fields-staticAmbient`: TS1206 on a decorated
  `declare` field and then TS1039 from `checkAmbientInitializer`, which
  upstream reaches only through `checkGrammarProperty` behind
  `!checkGrammarModifiers(node)`.
- `autoAccessorDisallowedModifiers`: TS1191/TS1120 on `accessor import …`,
  where upstream's `findFirstIllegalModifier` … chain reports TS1275 and the
  callers' `!checkGrammarModifiers(node)` silences the import/export arms.

**Port.** `check_grammar_modifiers` (`grammar.rs`) is
`checkGrammarModifiers` (`grammarchecks.go:214`) arm for arm:
`reportObviousDecoratorErrors`, `reportObviousModifierErrors`
(`findFirstIllegalModifier`), the `this`-parameter test, the decorator
position arms (TS1206, TS1249, TS1207, TS8038), the per-keyword switch in
upstream's `else if` order, the constructor, import and parameter-property
tails, and `checkGrammarAsyncModifier`. Every ten-function piece above is
deleted. The callers that upstream gates on the answer read it:

- `checkImportDeclaration` TS1191, `checkExportDeclaration` TS1193 (new) and
  `checkExportAssignment` TS1120, each `!checkGrammarModifiers(node) &&
  node.Modifiers() != nil` on the first token;
- `checkGrammarProperty`, which now carries its `checkAmbientInitializer` arm
  (TS1039/TS1254, `grammarchecks.go:1921`) and the TS1276 arm
  (`:1894`) instead of the dispatch calling the former ungated;
- the existing readers of `modifier_chain_reported`.

**Kinds.** `grammar_modifier_nodes` names the kinds whose checker calls the
function (`checkTypeParameter` … `checkTypeAliasDeclaration`,
`checkGrammarFunctionLikeDeclaration`, `checkGrammarIndexSignature`,
`checkGrammarClassDeclarationHeritageClauses`). Object-literal members are
not among them: their modifiers are `checkGrammarObjectLiteralExpression`'s
(`check_grammar_object_literal_modifiers`), except that a method or accessor
in an object literal is a function-like and gets both, as upstream's does.
The old tail walked every node with modifiers, including property
assignments.

**Once per node.** Upstream calls the function from each gated checker and
reports again each time; `SortAndDeduplicateDiagnostics`
(`compiler/program.go:1454`) folds the repeats. This port's collection keeps
every report, so `modifier_chain_checked` (a `Checker` field replacing
`decorator_error_reported`) memoises: key is the declaration `NodeId`, owner
the per-file check walk's `Checker`, published once on first call; the answer
is `modifier_chain_reported.contains(node)`. No new traversal: the work is one
pass over the node's modifier list, plus the binder's declaration list for the
legacy accessor arm and an ancestor walk for `NodeFlagsAmbient`.

**`NodeFlagsAmbient`.** This parser never sets the flag. `has_ambient_flag`
walks ancestors for what `parseDeclaration` (`parser.go:1128`) and
`parseClassElement` (`:1878`) set it on: a declaration file, or anything
inside a statement-level declaration or class property/method whose modifiers
include `declare`. The three older approximations (`declaration_is_in_an_
ambient_context`, `is_in_ambient_context_for_overloads`,
`declaration_has_ambient_flag`) differ from this and from each other; they are
other lanes' and were left alone.

**Reparsed JSDoc modifiers.** Upstream's list carries them after the written
ones with `NodeFlagsReparsed`. `check_jsdoc_reparsed_modifier_grammar`
(`jsdoc_modifiers.rs`, r7-reports) already walks them with the arms a
reparsed modifier can reach; the chain hands it its state after the written
walk and treats a report from it as its own `return true`. Folding those arms
into the one switch would be more literal and is left to that file's owner.

**`await using`.** This parser eats the `await` of `await using` and flags
the list `Using` only, where `parseVariableDeclarationList` makes it
`NodeFlagsAwaitUsing` (`parser.go:1563`). The chain reads the block-scope kind
through `is_await_using_list` (`using_declaration.rs`), which recovers the
`await` from the source text. Proposed to r7-parser: flag the list
`CONST | USING`, after which both reads become a flag test.

**Not ported here.** TS8038/TS1206 in a `.js` file
(`esDecorators-classDeclaration-exportModifier`) are the parser's
(`parser.go:6696`, `checkJSSyntax`), not the checker's: a plain JS file's
checker diagnostics are reduced to `plainJSErrors`, which lists neither code.

**Falsifier.** A modifier diagnostic at a different position or code than
upstream on a node kind `grammar_modifier_nodes` names, or a gated grammar
check (TS1191/TS1193/TS1120/TS1039/TS1276 …) reported on a node that also has
a modifier report.

## §2 `checkGrammarAccessor` (GRAMMAR-ACCESSOR-DECLARATION)

**Forcing fact.** r6-triage's row: 8 diagnostics cases. TS1094 (type
parameters), TS1095 (set return annotation) and TS1052 (set parameter
initializer) had no emitter, TS1183 on an accessor body in a type literal or
interface was missing, and TS1049/TS1054 counted a `this` parameter
(`thisTypeInAccessors`).

**Port.** `check_grammar_accessor_declaration` (`grammar.rs`) is
`checkGrammarAccessor` (`grammarchecks.go:1307`) arm for arm: the missing-body
`'{' expected` arm behind `NodeFlagsAmbient` (`has_ambient_flag`, §1), TS1318
and TS1183 on a body, TS1094, `doesAccessorHaveCorrectParameterCount` with
`getAccessorThisParameter` (`checker.go:19931`: one more parameter than the
accessor takes when the first is `this`), TS1095, and the value parameter's
TS1053/TS1051/TS1052 (`GetSetAccessorValueParameter`, the parameter after a
`this` one). `check.rs`'s `check_grammar_accessor` keeps only its caller's
shape: `!checkGrammarFunctionLikeDeclaration && !checkGrammarAccessor`, of
whose first conjunct `checkGrammarModifiers` is consulted (the parameter-list
conjunct reports from its own dispatch, as before).

**Limitation accepted.** `funcData.TypeParameters != nil` is true for an empty
`<>` list; this tree keeps an empty list as no list, so `get x<>()` gets no
TS1094. The parser lane owns the representation (TS1098/TS1099 have the same
gap, `GRAMMAR-EMPTY-TYPE-PARAMETER-OR-ARGUMENT-LIST`).

## §3 TS1164 on a computed enum member name (ENUM-MEMBER-COMPUTED-NAME)

`computeEnumMemberValue` (`checker.go:23958`) reports
`Computed property names are not allowed in enums` with `c.error` on
`member.Name()` when `ast.IsComputedNonLiteralName`; a computed name whose
expression is a string, no-substitution template or numeric literal is named
by its text and goes through the numeric-name arm (TS2452). Ported in
`check_enum_member_name`. Upstream's `checkEnumMember` never checks the name, so its expression is
never resolved; this port's check walk resolves every value identifier, so
`[e]` drew TS2552/TS2304. The integrator granted `name_slots.rs`'s
`names_in_unchecked_region`, which now has a third arm: a
`ComputedPropertyName` whose parent is an `EnumMember` is a region native
never checks. With it the five cases convert (`parserComputedPropertyName16`,
`26`, `30`, `34`, `parserES5ComputedPropertyName6(target=es2015)`).

## §4 `tsr-2zk.1258`: two diagnostics reported from the wrong checker

r7-perf's materialised multi-file run (`docs/parity/notes/r7-perf.md` §3 on
`box/r7-perf`) found two cases whose diagnostics drop under `--checkers 4`,
because a checker that does not own the target file reported them and the
owner never did. Both score RIGHT in the single-checker harness.

- **TS2300 `prototype`** (`mergedClassWithNamespacePrototype`). Upstream has
  two producers: the binder when the class and the namespace are in one file
  (`bindClassLikeDeclaration`, `binder.go:962`-`:965`), and
  `mergeSymbolTable` → `reportMergeSymbolError` (`checker.go:14201`) when the
  merge crosses files, which every checker runs from `initializeChecker`. The
  port reported both from the class's check. The cross-file arm is now
  `report_class_prototype_merge_conflicts` (`merge_conflicts.rs`), which runs
  in every checker's merge report; the binder neither mints the synthetic
  `prototype` property nor records the pair, so it scans the global table for
  a class symbol whose exported `prototype` is declared in a file none of the
  class's declarations is in (one pass per checker). It takes
  `reportMergeSymbolError`'s message choice from the source's flags (the
  synthetic target is `Property | Prototype`). The same-file arm stays at the
  class (`check_merged_namespace_prototype`).
- **TS2813/TS2814** (`duplicateIdentifiersAcrossFileBoundaries`).
  `checkFunctionOrConstructorSymbolWorker` runs once per symbol per checker
  from whichever declaration that checker reaches first
  (`links.functionOrConstructorChecked`, `checker.go:3463`); the port ran the
  class-merge arm only from the symbol's first declaration. The link is now
  `class_function_merge_checked` (a `Checker` field keyed by merged
  `SymbolId`, set on first visit).

**Evidence.** Both cases materialised under `/tmp/mt` and run through the
release CLI: `--singleThreaded` and `--checkers 4` now print the same
diagnostics, identical to `target/tsgo-pinned`'s; the base binary drops
`file2.ts(3,10)` TS2814 and `file2.ts(4,7)` TS2813 under `--checkers 4`.

## §5 `checkGrammarImportClause` and the type-only specifier arms (IMPORT-ATTRIBUTES remainder)

r6-triage left three cases of `IMPORT-ATTRIBUTES-CHECKS` that are other
operations: `grammarErrors` (TS1363, TS1392) and `importSpecifiers1`
(TS2206). Ported in `grammar.rs`:

- `check_grammar_import_clause` is `checkGrammarImportClause`
  (`grammarchecks.go:2118`) whole: the `type` arm (TS1363 for a default plus
  named bindings, else `checkGrammarTypeOnlyNamedImportsOrExports`, TS2206 on
  the first specifier's first token) and the `defer` arm (TS18058/TS18059/
  TS18060), which was `check.rs`'s `check_deferred_import_clause` and moves
  here unchanged. The clause range starts at the phase modifier upstream; this
  parser leaves the modifier outside the clause's span, so the range is
  rebuilt from it (the old note §615's workaround, kept).
- `check_grammar_export_declaration` is `checkGrammarExportDeclaration`
  (`grammarchecks.go:196`): TS2207 for `type` on a specifier of
  `export type { … }`.
- `check_grammar_import_equals_type_only` is `checkImportEqualsDeclaration`'s
  TS1392 arm (`checker.go:5491`) for an entity-name alias.

**Gate not mirrored.** Upstream reaches the clause only when
`checkExternalImportOrExportDeclaration` passed, and skips the bindings when
the clause reported. This port's arms of that function report from their
own dispatch and its binding checks are not gated on the clause; as with the
deferred arm before, the clause is checked unconditionally. No corpus case
pairs a clause error with a binding or attribute diagnostic.

**JSDoc `@import`.** `importTag15` waits on the JSDoc lane: the reparsed
`@import` is an import declaration upstream, which this port's check walk
does not visit.

## §6 TS2312 in `resolveBaseTypesOfInterface` (CHECK-INTERFACE-HERITAGE-AND-BASES, first arm)

`resolveBaseTypesOfInterface` (`checker.go:19498`) reports
`An interface can only extend an object type or intersection of object types
with statically known members` at an `extends` element whose reduced type is
not an error and not a valid base type. `base_types.rs` computed the list but
reported nothing. The report is now made where upstream makes it: the
resolution runs once per symbol per checker (`base_type_links`, its module
documentation's publication rules), and every caller that resolves an
interface's bases reaches it, as upstream's do. The circular arm
(`reportCircularBaseType`) stays `check.rs`'s `check_recursive_base_type`;
moving it here would report TS2310 a second time beside that check, and this
port's collection does not deduplicate.

`interface_heritage_type` gains `getTypeReferenceType`'s type-parameter arm:
an unconstrained `T` resolved to the error type, so `interface I<T> extends T`
was silent; it is now the parameter's declared type (with type arguments it
stays the error type, upstream's TS2315 path). Other element shapes this
partial `getTypeFromTypeNode` cannot type still read as the error type, so
the report is a subset of upstream's, never a superset.

Converted: `typeParameterAsBaseType`. `interfaceExtendsObjectIntersectionErrors`
gains its two TS2312 lines and stays WRONG on the heritage relation
(TS2430/TS2416/TS2411/TS2413 against alias and intersection bases).

## §7 `reportUnused` has no parse-error gate

`check_unused_identifiers` dropped every unused-identifier report in a file
with parse diagnostics, as a stand-in for `reportUnused`'s
`location.Flags & NodeFlagsThisNodeOrAnySubNodesHasError` test
(`checker.go:7092`). At the pin nothing writes that flag
(`ast/nodeflags.go:24` declares it; the parser sets only
`NodeFlagsThisNodeHasError`, `parser.go:5908`), so upstream reports unused
identifiers in a file with parse errors. The gate is removed; the declaration
file exemption (`checkSourceFile`, `checker.go:2221`) stays.
`unusedLocalsAndParameters` now matches its 21 TS6133/TS6196 lines and stays
WRONG on one extra TS6133 inside the parser-recovered `for (let x: y)`, whose
recovery differs from upstream's (the parser lane's).

## §8 `checkGrammarDecorator` (TS1497)

`check_grammar_decorator` (`grammar.rs`) is `checkGrammarDecorator`
(`grammarchecks.go:127`): outside the decorator grammar's parenthesized, call
and member forms, TS1497 on the decorator's expression with TS1498 related at
the first offending node (the earliest `?.`, an inner call, or a
non-identifier root; non-null assertions and instantiation expressions are
walked through). Upstream reaches it from `checkDecorators` → `checkDecorator`
for each decorator of a declaration that `NodeCanBeDecorated` accepts;
`check_decorators_grammar` is that entry and is called beside the existing
`markLinkedReferences` port at the top of the check walk (one line in
`check_node_worker`). The decorator call resolution `checkDecorator` goes on
to is the calls lane's. Verified against `target/tsgo-pinned` on a scratch
file; `esDecorators-decoratorExpression.1` still has parse errors on this
base and converts once r7-parser's decorator-expression parsing lands.

## §9 `checkGrammarAwaitOrAwaitUsing` on `NodeFlagsAwaitContext` (CHECK-GRAMMAR-AWAIT-YIELD-CONTEXT)

**Forcing fact.** The function's arms were spread over three stand-ins:
`check_await_in_non_async_function` and `check_for_await_context`
(`check.rs`), which read "the containing function has no `async` modifier"
in place of `NodeFlagsAwaitContext` and special-cased property initializers
(§613), `check_await_in_parameter_initializer` (`check.rs`), and
`check_top_level_await` (`module_format.rs`), whose top-level arm ran with no
`AwaitContext` test at all, so a top-level `await` the parser reparses
reported TS1378. The class-static-block arms (TS18037, TS18054, TS18038) had
no emitter. r7-parser `1321f0aa` stamps `NodeFlags::AWAIT_CONTEXT` on every
node finished in an await context (`finishNode`'s `contextFlags`), which is
the flag the function reads.

**Port** (`module_format.rs`):
- `check_grammar_await_or_await_using` is the function whole, for an
  `AwaitExpression` (`checkAwaitExpression`, `checker.go:10846`) and for an
  `await using` list: the static-block arm (`c.error`, no parse-error gate),
  the `AwaitContext` gate with the top-level arms (`check_top_level_await`)
  and the TS1308/TS2852 arm with TS1356 related at a non-constructor,
  non-`async` container, and TS2524.
- `check_for_await_grammar` is `checkGrammarForInOrForOfStatement`'s
  `for await` arms behind the same gate (top level, or TS1103 with TS1356
  related at `GetContainingFunction`) and `checkForOfStatement`'s TS18038.
- `checkGrammarVariableDeclarationList`'s last step: `grammar.rs`'s list
  check now ends in `check_await_using_declaration_list` when no earlier arm
  reported. In a file with parse diagnostics every earlier arm (and the
  `checkGrammarModifiers` gate) is a silent `false` upstream, so the tail
  always runs there; this port skips its list check wholesale in such a file,
  so `module_format.rs` asks the tail directly for statement and `for`-head
  lists in that case.
- The `await` of `await using` is eaten by this parser, so its position is
  recovered as before (`await_using_keyword_start`, or the token in front of
  a `for`-head list).
- The dispatch (`check_await_grammar_of`) runs ahead of
  `check_module_format`'s `module: none` return: the grammar is not a
  module-format check, and the top-level arm reads the module kind itself.

**`binder.go:1311`'s TS1359 arm** (`await` as an identifier in an
`AwaitContext`, `strict_mode.rs`'s `check_contextual_identifier`) was held
at first: measured on `d3a34908` at 15 cases gained and 8 lost, because this
parser's `parse_type` kept the await and yield contexts (`parseType`'s
`setContextFlags(NodeFlagsTypeExcludesFlags, false)`, `parser.go:2608`), so
`var v: await` inside an async function drew TS1359
(`asyncFunctionDeclaration13`, `asyncArrowFunction10`,
`parser.asyncGenerators.*`). r7-parser `c83bb41b` clears them in types;
re-measured on `cd4afbcb` plus this lane's commits it is +2
(`classStaticBlock22` es2022, esnext), 0 lost, and lands. The `yield` arm
after it is unreachable upstream: `yield` is the last future reserved word
(`KindLastFutureReservedWord`), so the strict-mode arm before it takes it.

`topLevelAwaitErrors.1`'s TS2863 on `string` in `class C extends await<string>`
was first read as a `check.rs` decline; r7-parser's dump of native's tree shows
`string` is the class's only base expression, and r7-parser `f44a4956`
converts the case. Nothing here.

## §10 A private set accessor is checked for use

`checkUnusedClassMembers` (`checker.go:7115`) skips a set accessor only when
its symbol also has `GetAccessor` ("Already would have reported an error on
the getter"); `check_unused_class_members` skipped every set accessor, so a
lone private setter was never reported (`unusedSetterInClass`). And
`markPropertyAsReferenced` (`checker.go:27718`) drops a write-only access
*unless the property is a set accessor*, so a write keeps a setter used where
it leaves a field unread. The member-name pass records write-only accesses
under their own key (`member_write_key`), read only for set accessors, with
the same self-access rule as reads. Converted: `unusedSetterInClass(target=es2015)`.

## §11 `#x in obj` reads `#x`

`checkPrivateIdentifierExpression` (`checker.go:7837`), which `#x in obj`
reaches for its left operand, calls `markPropertyAsReferenced` with no
write-only node and `isSelfTypeAccess` false, so a private member used only
as a brand check is not unused. The member-name pass (`note_member_name_at`)
now records a private identifier that is a binary expression's operand.
Converted: `privateNameInInExpressionUnused` (es2022, esnext).

## §12 `export { undefined }` resolves before it is judged global

`checkExportSpecifier` (`checker.go:5563`) resolves the exported name and
reports TS2661 when the symbol *is* `c.undefinedSymbol` or
`c.globalThisSymbol`, or is declared in a script. `check_export_specifier_is_local`
(`meaning_mismatch.rs`) took the spellings `undefined` and `globalThis` as
global before resolving, so a module's own `var undefined` re-exported drew
TS2661 (`reExportUndefined2`). It now resolves first: the binder's synthesised
`undefined` symbol is the global one; an unresolved `globalThis` still stands
for upstream's `globalThisSymbol`, which this port does not synthesise.

## §13 `checkGrammarModuleElementContext` for every caller (CHECK-MODULE-ELEMENT-CONTEXT-GRAMMAR)

`check_grammar_module_element_context` (`grammar.rs`, replacing `check.rs`'s
namespace-only version) is `checkGrammarModuleElementContext`
(`grammarchecks.go:206`) with each caller's message:
`checkModuleDeclaration` (TS2435 for an ambient module, TS1235 for a
namespace; the ambient arm had been declined), `checkImportDeclaration` and
`checkImportEqualsDeclaration` (TS1232, TS1473 in JavaScript),
`checkExportDeclaration` (TS1233, TS1474 in JavaScript), on the statement's
first token. It answers `true` in an illegal context whether or not parse
diagnostics silence the report, and each caller bails out. This port's
callers that sit behind the bail-out now read it: TS1191/TS1193 and the
declaration's grammar (`checkGrammarImportClause`, `checkGrammarExportDeclaration`,
TS1392), `check_export_declaration_in_namespace` (TS1194) and the specifier
checks (`check_export_specifier_is_local`, TS2661). Other checks this port
runs on such a declaration from elsewhere in the walk (module resolution,
alias checks) are not gated; no corpus case pairs them with an illegal
context. Converted: `moduleElementsInWrongContext`,
`moduleElementsInWrongContext3`.

## §14 TS2844 for a constructor-local reference in a property's type

`checkAndReportErrorForInvalidInitializer` (`checker.go:1514`) chooses
`Type of instance member variable '{0}' cannot reference identifier '{1}'
declared in the constructor` (TS2844) over the initializer message (TS2301)
when the reference sits inside the property's type annotation
(`prop.Type.Loc.ContainsInclusive(errorLocation.Pos())`, `:1522`).
`check_value_identifier` (`check.rs`, granted) reported TS2301 for both.
Converted: `initializerReferencingConstructorParameters`.

## §15 An interface's `extends` element is a type reference

`checkInterfaceDeclaration` (`checker.go:5023`) runs `checkTypeReferenceNode`
on each `extends` element, so a namespace there is TS2709 and a value TS2749
(`moduleAsBaseType`). The type-name reporter in `check.rs` (granted) admitted
an `ExpressionWithTypeArguments` only under a class's `implements` clause; it
now admits an interface's `extends` clause too. A class's `extends` stays a
value (`checkClassLikeDeclaration` checks its expression) and an interface's
`implements` stays TS1176 only. A primitive spelling there (`interface x
extends string`) is native's TS2840 alone; `check_value_identifier` already
reports it, so the type path returns for the six primitive names in any
heritage clause, reporting only the `implements` message itself (first
landing lost `errorLocationForInterfaceExtension` and
`interfacedeclWithIndexerErrors` to a doubled TS2552; batch 13 refused it). Matches `target/tsgo-pinned` on a scratch file
covering a namespace, an unresolved name (TS2304, not doubled) and a value.
Converted: `moduleAsBaseType`.

## §16 TS18011: `delete` of a private name

`checkDeleteExpression` (`checker.go:10811`) reports
`The operand of a 'delete' operator cannot be a private identifier` for a
property access whose name is a private identifier, then goes on to its
symbol arms. `delete_operand.rs` (granted) had only the symbol arms.
Converted: `privateNamesNoDelete`.

## §17 `checkGrammarYieldExpression` on `NodeFlagsYieldContext`

With the parser stamping `NodeFlagsYieldContext` and building a
`YieldExpression` only where `isYieldExpression` does (r7-parser `f44a4956`),
`check_grammar_yield_expression` (`grammar.rs`) is
`checkGrammarYieldExpression` (`grammarchecks.go:1777`) whole: TS1163 on the
`yield` keyword without the flag, and TS2523 (`c.error`) in a parameter
initializer. It replaces `check.rs`'s `check_yield_grammar` (an ancestor walk
for a generator with three operand-shape bounds standing in for the parser's
yield context) and `check_yield_in_parameter_initializer` (the same bounds).
A decorator is parsed in its class's enclosing context, so
`@(yield "")` in a generator is legal (`generatorTypeCheck59`), and an enum
member initializer is parsed outside it (`awaitAndYield`). Converted:
`awaitAndYield`, `generatorTypeCheck59`.

## §18 `checkGrammarIndexSignatureParameters` on types (CHECK-GRAMMAR-INDEX-SIGNATURE-PARAMETERS)

`check_grammar_index_signature_parameters` (`grammar.rs`) is
`checkGrammarIndexSignatureParameters` (`grammarchecks.go:796`) arm for arm,
behind `checkGrammarIndexSignature`'s `checkGrammarModifiers`. Its type arms
read `getTypeFromTypeNode` of the parameter's annotation: a literal,
unique-symbol or generic type is TS1337 (`someType(t,
StringOrNumberLiteralOrUnique) || isGenericType(t)`), one that is not
every-way `isValidIndexKeyType` TS1268, and a signature without its own
annotation TS1021. It replaces `index_signature_parameter_shape_error`
(`grammar.rs`) and `check.rs`'s `check_index_signature_key_type` and
`check_index_signature_parameter_type`, which judged the written annotation
syntactically (keyword lists and a type-alias test) and had no TS1337. A type
this port cannot build answers nothing for the two type arms (`is_gap`).
Not ported: the parameter list's trailing-comma arm (TS1025,
`indexSignatureWithTrailingComma`), which needs the parser to record a
parameter list's trailing comma. Converted: `indexerConstraints2`,
`genericIndexTypeHasSensibleErrorMessage`.

## §19 `for (await using x in …)` is TS1494

`checkGrammarVariableDeclarationList` (`grammarchecks.go:1658`) reports a
`using` list in a `for…in` head as TS1493 and an `await using` one as TS1494,
on the list — which starts at the `await` upstream. The arm declined both,
since this parser eats the `await`; `await_using_list_await_start`
(`module_format.rs`, §9) recovers it, so both messages are reported with the
list's upstream start. Converted: `awaitUsingDeclarationsInForIn`.

## §20 `checkGrammarTaggedTemplateChain` (TS1358)

`check_grammar_tagged_template_chain` (`grammar.rs`) is
`checkGrammarTaggedTemplateChain` (`grammarchecks.go:859`), called first from
`checkTaggedTemplateExpression` (`checker.go:10035`, one line in the
dispatch): a tagged template with a `?.` token or inside an optional chain
(`NodeFlagsOptionalChain`) is TS1358 on the template. It needed r7-parser
`6b560647`, which parses a template after `?.` as a tagged template. Its
`checkGrammarTypeArguments` sibling is not ported. Converted:
`taggedTemplateChain`.

## §21 TS2790 reads the receiver's member, and the `exactOptionalPropertyTypes` arm

`checkDeleteExpressionMustBeOptional` (`checker.go:10825`) tests
`getTypeOfSymbol` of the property the access resolved, and under
`exactOptionalPropertyTypes` the symbol's `Optional` flag instead of the
type's `undefined` fact. `delete_operand.rs` read the source declaration's
type, so a mapped receiver's optional member (`Partial<Foo>`'s `a`, whose
transient symbol this port does not mint) drew TS2790; the type is now the
receiver's member type (`get_type_of_property_of_type`). The exact arm reads
the declaration's `?` (`property_is_optional`) when the member is the
declaration's own, reached through its declared parent; a mapped or
instantiated member's optionality is the receiver's
(`identity_mapped_source_property_is_optional`, private to `members.rs`), so
that case declined — routed as a visibility request. *Superseded:* with the
integrator's grant (visibility only) the arm asks
`identity_mapped_source_property_is_optional(receiver, name)` for every
member, which is the receiver-level `Optional` flag, and no longer declines. Converted:
`deleteExpressionMustBeOptional_exactOptionalPropertyTypes` (exact true and
false).

## §22 `checkGrammarBindingElement` (GRAMMAR-BINDING-PATTERN-REST)

`check_grammar_binding_element` (`grammar.rs`) is `checkGrammarBindingElement`
(`grammarchecks.go:1536`), first in `checkBindingElement` (`checker.go:4274`,
one line in the dispatch): a rest element not last in its pattern is TS2462,
one with a property name TS2566 on the name, one with an initializer TS1186
on the `=` (`Initializer.Pos()-1`; this tree's spans start after trivia, so
the `=` is found by skipping back over it in the source). Not ported: the
trailing-comma arm (TS1013), which needs the parser to record a binding
pattern's trailing comma. Converted: `restElementWithInitializer1`,
`objectBindingPattern_restElementWithPropertyName`.

## §23 `using` declarations in an ambient context (TS1545/TS1546)

`checkGrammarVariableDeclarationList`'s ambient arm (`grammarchecks.go:1662`):
a `using` / `await using` list with `NodeFlagsAmbient` is TS1545 / TS1546 on
the list (from the `await` for the latter). Read through `has_ambient_flag`
(§1). The arm returns, so the `await using` tail (§9) no longer adds TS2852
there. Converted: `usingDeclarations.16`, `awaitUsingDeclarations.16`.

## §24 `await using` read from the list's flags

r7-parser `054b714f` flags an `await using` list `CONST | USING`
(`NodeFlagsAwaitUsing`) and starts its span at the `await`, as
`parseVariableDeclarationList` does (`parser.go:1563`). The recoveries §9,
§19 and §23 describe — the `await` found as the gap before a statement's
list, or as the token before a `for`-head list (`is_await_using_list`'s
source-text test) — are gone: `await_using_list_await_start`
(`module_format.rs`) is the flag test and the list's start,
`await_using_keyword_start` and `is_await_using_statement` are deleted, and
`checkGrammarModifiers`' block-scope kind and the case-clause arm read the
flags directly. Correction of the record: those sections' "this parser eats
the `await`" no longer holds as of `054b714f`.

## §25 `checkGrammarArrowFunction` (TS1200) and the accessor-pair arm (TS2676/TS2808)

**TS1200.** `checkGrammarArrowFunction` (`grammarchecks.go:772`) is the
fourth arm of `checkGrammarFunctionLikeDeclaration`'s `||` chain (`:758`):
modifiers, then the type-parameter list, then the parameter list, then the
arrow. `check_grammar_arrow_function` (`module_format.rs`) is the whole Go
function: the TS7060 `.mts`/`.cts` arm, which used to stand alone in
`check_module_format` behind its `module: none` return, and the line-break
arm. Upstream reports TS7060 and does **not** return
(`TODO(danielr): should we return early here?`), so the line-break arm still
runs. The check walk calls it only when `check_grammar_parameter_list`
(`check.rs`, which now returns whether it reported) reported nothing.
`checkGrammarModifiers` is asked inside it, memoized as §1 describes.
TS1098, the empty-list arm, has no producer: the tree's `&[]` cannot tell
`<>` from no list. That is routed to r7-parser, with TS1099.

*Forcing constraint.* Upstream tests the `=>` token's **full** text
(`Pos()`..`End()`) for a line break. That text is its leading trivia, and a
multi-line comment counts (`(x) /*\n */ => x` is TS1200). This tree's token
spans start after trivia, and no per-token scanner flags are stored. So the
trivia is re-scanned. The scan starts at the end of the last child written
before `=>`, which is a token boundary: the return type, else the last
parameter, type parameter or modifier, else the arrow's own start. The
scanner runs forward to the token at the `=>`'s start and reads its
`PRECEDING_LINE_BREAK`, which the scanner sets for a break anywhere in the
trivia.
*Alternatives.* (a) Step backwards from `=>` over whitespace, as
`full_start_before` does. That cannot step over a comment, and `//` comments
make a backward scan ambiguous. (b) Have the parser store a
preceding-line-break flag on the token. That is cleaner, and it would win if
the parser started recording token flags for other checks; it is r7-parser's
file. The forward scan reads at most the `)`/`,`/`>` tokens between the last
child and `=>`, once per arrow, on the walk's single visit. It is not cached.
*Falsifier.* A TS1200 at an arrow whose `=>` has no line break before it, or
a missing one where it does. `disallowLineTerminatorBeforeArrow` covers
comments, return types, rest/optional/default parameters and nesting.

**The parameter list returns at TS1015.** Upstream's
`checkGrammarParameterList` returns `true` at a `?` parameter with an
initializer (`grammarchecks.go:711`). That ends the loop, so no TS1016
follows it, and the `||` chain reads it. TS1015 itself is still reported once
per list by `check_optional_parameter_initializer`; the loop now returns
`true` at that parameter.

**TS2676/TS2808.** `checkAccessorDeclaration`'s accessor-pair arm
(`checker.go:2954`) takes the member symbol's first get and first set accessor
(`GetDeclarationOfKind`). Differing `abstract` is TS2676, and a `protected`
getter with a setter neither `protected` nor `private`, or a `private` getter
with a non-`private` setter, is TS2808. Each is reported on both names.
Upstream checks the pair once, from whichever accessor is checked first,
behind the getter's `NodeCheckFlagsTypeChecked` link.
`check_accessor_pair_modifiers` (`grammar.rs`) instead reports on each
accessor's **own** name when that accessor is checked. The walk checks both,
and diagnostics are per file, so the set is the same and there is no link to
own. The only divergence is an accessor that is never checked (a library
declaration merged with a user one). Upstream's diagnostic for it lands in a
file whose diagnostics are not collected, so dropping it changes nothing.
`ModifierFlags()` includes the JSDoc modifiers the reparser hosts
(`has_effective_modifier`). A dynamic computed name is declined: a
late-bindable one pairs only through the late-bound member symbol, which this
checker does not build (r7-reports §3).
*Falsifier.* A TS2676/TS2808 on one accessor's name but not its partner's in
a checked file, or a set that differs between `--singleThreaded` and
`--checkers 4`.

**Deferred: `checkTupleType`'s element grammar** (TS1257/1265/1266, TS2574).
The order arms are syntactic. The variadic arm needs `isArrayLikeType`, which
answers `false` on an undecided relation (so TS2574 could be reported
falsely), and `isArrayType || isTupleType && combinedFlags&Rest` over this
checker's print-only variadic tuples (`declared.rs` §40). It would convert
one single-code case (`optionalTupleElements1`) for that risk.

## §26 `checkKindsOfPropertyMemberOverrides`' abstract half (TS2515/2650/2653/2654/2655/2656)

`check_inherited_abstract_members_implemented` (`heritage_conformance.rs`) is
the `notImplementedInfo` pass of `checkKindsOfPropertyMemberOverrides`
(`checker.go:4536`-`:4590`, reported at `:4664`). It runs first in the
existing `check_kinds_of_property_member_overrides`, the port of the same Go
function's property-kind half, which the check walk already calls for every
node. It replaces check.rs's `check_abstract_members_implemented`, a
declaration walk that built only the single-member form (TS2515) and ignored
interface merges. That stand-in reported two false TS2515s in
`mergedInheritedMembersSatisfyAbstractBase`.

*The algorithm, as upstream.* It applies to a class-like with an `extends`
clause whose `getBaseTypes(t)[0]` resolved (`checkClassLikeDeclaration`,
`checker.go:4313`), and covers class expressions too. For each property of
the base type, it asks whether the class type's property of that name has the
same target symbol and whether its declaration is `abstract`. The declaration
is the get accessor for an accessor symbol, else the value declaration
(`getDeclarationModifierFlagsFromSymbol`), with the reparser's JSDoc
modifiers. If both hold, the member is missing, unless the class is
`abstract` or another base type supplies a different symbol of that name (an
interface merged with the class). The list is reported at the class name, or
at a class expression. One member is TS2515/TS2653 with the name unquoted;
two to five are TS2654/TS2656 with every name quoted; more than five are
TS2655/TS2650 with four quoted and "and N more". Names are
`TypeToString(t)` and `TypeToString(baseType)` (`C<T>`, `A<number>`).

*Target symbols.* Property symbols here are the declared ones for every
instantiation, so `getTargetSymbol` is the identity. The exception is a
member inherited through a generic heritage entry (`extends A<T>`), which
`get_property_of_type` leaves as a gap (members.rs §202).
`generic_heritage_property_symbol` answers its declaring symbol, which is
exactly the target, and only identity and flags are read from it
(`target_property_of`).

*Declines.* The check reports nothing when the base's property list is not
certified (`get_property_names_of_type` answers `None`), and nothing for a
class whose missing member has a non-identifier name: `symbolToString` of a
string, numeric or computed name has spellings this port does not produce.
`builtinIterator`'s lib `Iterator` base is still declined (no TS2515 there,
which leaves that case's existing gap).

*Cost and ownership.* No new cache. The walk visits each class once. The base
property list is `get_property_names_of_type`'s existing shared memo, and
each name costs up to three property lookups.

*Falsifier.* A TS2515-family report on a class that implements the member
through a merged interface or an intermediate base. A wrong member order
(the order of the base's `getPropertiesOfType`: own declarations, then
inherited). A count that differs between `--singleThreaded` and
`--checkers 4`.
Converted: `abstractPropertyNegative` (with §25), `classAbstractGeneric`,
`classExpressionExtendingAbstractClass`, `classAbstractInheritance2`.

## §27 Unused `@template` type parameters in JS (TS6133/TS6196/TS6205)

`checkUnusedTypeParameters` (`checker.go:7290`) reads `node.TypeParameters()`.
In a JS file the reparser hosts a comment's `@template` list there when the
written list is empty (`reparseHosted`'s `KindJSDocTemplateTag` arm,
`parser/reparser.go:446`). `type_parameters_of` (`unused.rs`) took only the
written list, so `/** @template T */ function f() {}` reported nothing where
upstream reports TS6196 on `T`. It now falls back to `jsdoc_template_lists`
for the comment's host, the same list `checkTypeParameters` already reads for
these nodes (check.rs). The host is the node itself, or the ancestor (at most
four levels up: a variable statement, property assignment, expression
statement) whose comment `jsdoc_function_like_host` maps to this node, which
is the forward direction of that mapping. A list is used only when its kind
is the node's kind, so a `@typedef`'s `@template` list (kind
`TypeAliasDeclaration`) never lands on a function. References to the
parameter are the existing `symbol_reference_kinds`, so a parameter the
binder does not bind is not reported. No cache; this runs once per checked
declaration in the unused pass.
*Falsifier.* A TS6133/TS6196 on a `@template` parameter that a JSDoc type
references, or on a function that has a written `<T>` list.
Converted: `unusedTypeParameters_templateTag`.

## §28 TS2310 from the base-type resolutions

TS2310 (`Type '{0}' recursively references itself as a base type.`) now comes
from the three places upstream reports it, all in `base_types.rs`:

* `getBaseTypes`' failed `popTypeResolution` (`checker.go:19189`) reports at
  every class or interface declaration of the symbol. This is the arm a
  type-level cycle reaches: `class Foo extends NextType<Foo>`, where
  `NextType`'s default `T = C['someProp']` needs `Foo`'s members while `Foo`'s
  base types are being resolved (TypeScript#16861,
  `circularConstraintYieldsAppropriateError`). `Resolutions::pop` already
  answered whether the frame stayed cycle-free; `get_base_types` discarded it.
* `resolveBaseTypesOfInterface`'s `t == baseType || hasBaseType(baseType, t)`
  arm (`:19508`) reports at the interface declaration.
* `resolveBaseTypesOfClass`'s same test (`:19260`) reports at the class's
  value declaration.

It replaces check.rs's `check_recursive_base_type`, a reachability walk over
`extends` names that reported once per symbol at its first declaration. That
walk could not see type-level cycles. Its dispatch line now forces
`get_base_types` for each class and interface declaration
(`check_base_types_resolved`), as `checkClassLikeDeclaration` and
`checkInterfaceDeclaration` do.

*Dedupe.* A mutual interface cycle closes both the interface arm and the
failed pop for the same declaration. Upstream reports twice and
`SortAndDeduplicateDiagnostics` folds the pair. This collection does not
dedupe, so `BaseTypeLinks::circular_reported` (key: declaration; owner: this
checker; published once) skips the second report.

*Not ported.* `t.objectFlags &^= ObjectFlagsMembersResolved` after a cycle:
this checker's member tables are not published through that flag.

*Measured.* Syntactic cycles match tsgo line for line, including a merged
interface that reports at both declarations (the walk reported once).
`circularConstraintYieldsAppropriateError` still lacks its TS2310: this
checker does not force `Foo`'s members while instantiating `NextType`'s
default `C['someProp']`, so the failed-pop arm is not reached. That cycle is
the instantiation lane's to produce, and this arm reports it once it does.

*Falsifier.* A TS2310 count that differs from upstream on merged interface
declarations (one report per offending declaration), or a TS2310 on a class
cycle (that is TS2506's, from the base constructor type, before this
resolution runs).

## §29 TS2564's constructor arm on the flow query (CLASS-PROPERTY-INITIALIZER)

`checkPropertyInitialization` (`checker.go:4933`) reports TS2564 when
`constructor == nil || !isPropertyInitializedInConstructor(...)`. The second
disjunct (`:4977`) builds a `this.<name>` (or `this[<expr>]`), parents it to
the constructor, gives it the constructor's `ReturnFlowNode`, and asks
`getFlowTypeOfReferenceEx(reference, propType, getOptionalType(propType))`.
The property is initialized when `undefined` does not survive.
`check_property_initialization` (check.rs, granted) used to approximate this:
it declined every constructor whose body names `this.<name>` anywhere
(`checker-notes-diag2.md` §87), so a property assigned on only some paths, or
read before it is assigned, was silent.

r7-flow §15 built the query, `flow_type_of_this_property_at(constructor,
name, declared, initial)`, on its synthetic-access machinery.
`is_property_initialized_in_constructor` calls it with `propType` and
`get_optional_type(propType, false)` and tests `contains_undefined_type`, as
upstream does.

*Names.* Identifier and private names are passed as written. A computed name
is spelled when its expression is a string, numeric or no-substitution
template literal, which is the name upstream's `this[<expr>]` matches.
Upstream also handles other computed names (`this[sym]`); the query cannot
spell those, so they keep §87's decidable part (a body that never names it
cannot assign it). That is the one remaining approximation, and it only ever
declines.

*Cost.* One flow walk per uninitialized, non-optional property of a class
with a constructor, as upstream. No cache.

*Falsifier.* A TS2564 on a property every constructor path assigns (the
synthetic matcher missing an assignment form), or none on one a path leaves
unassigned. Converted: `detachedCommentAtStartOfConstructor2` (r7-flow's
prototype measured the same +1, 0 lost); the exact oracle also gains
`strictPropertyInitialization`.

## §30 TS1312, a shorthand `= initializer` outside destructuring

`checkGrammarObjectLiteralExpression`'s shorthand arm
(`grammarchecks.go:1040`): outside a destructuring pattern
(`checkObjectLiteral`'s `inDestructuringPattern = ast.IsAssignmentTarget(node)`,
here `assignment_target_kind(literal) == None`), a shorthand with an
`ObjectAssignmentInitializer` is TS1312 on the first token of the child before
the initializer, its `EqualsToken`. The arm does not return. This parser eats
the `=` without keeping it (`ShorthandPropertyAssignment::new(.., None, None,
initializer)`), so `report_shorthand_equals` scans one token from the end of
the last child before it (type, postfix token, else name). That is where the
parser found it. The scan is declined unless the token is `=`, and it is not
needed if r7-parser ever records the token. Upstream also checks such a
shorthand's **initializer** instead of its name
(`checkShorthandPropertyAssignment`, `checker.go:13689`), so no TS18004 is
reported for an unresolved name there. That arm is in check.rs's
unresolved-value walk and is routed. Until it lands,
`shorthandPropertyAssignmentsInDestructuring` (both variants) and
`objectLiteralErrors` keep an extra TS18004 beside the now-correct TS1312.


## §31 TS1341 and TS1368, a class constructor spelled as an accessor or generator

*TS1341* (`checker.go:2936`) is an ungated `c.error` in
`checkAccessorDeclaration`, right after the grammar block: an accessor named
by the identifier `constructor` in a class-like. It is reported from
`check_accessor_pair_modifiers` (grammar.rs), which the check walk already
calls once per accessor (§25), so it needs no new dispatch.

*TS1368* (`checker.go:2798`) is `checkMethodDeclaration`'s first block. When
`checkGrammarMethod` reports nothing, a generator method named by the
identifier `constructor` gets `c.error(name)`. An object-literal method is
checked by `checkObjectLiteralMethod` and never reaches it (tsgo is silent on
`{ *constructor() {} }`). The check walk calls
`check_method_named_constructor_is_not_generator` from the same `||`-chain
arm as §25's arrow check, i.e. only when `checkGrammarParameterList`
reported nothing. It asks the remaining arms that can answer for a class
member named by an identifier: `checkGrammarModifiers` (memoized), and
`checkGrammarForGenerator`'s ambient and overload conditions, whose reports
stay check.rs's. Every grammar arm answers `false` in a file with parse
diagnostics, and TS1368 then still stands, as upstream. Not consulted:
`checkGrammarForUseStrictSimpleParameterList` (TS1346/TS1347), which is not
ported. A generator constructor with a non-simple parameter list under a
`"use strict"` prologue would report TS1368 where upstream reports only the
use-strict error.
*Falsifier.* TS1368 on an object-literal method, or beside TS1222/TS1221 on
the same method. Converted: `constructorNameInAccessor`,
`constructorNameInGenerator`.

## §32 TS18012, `#constructor` (PRIVATE-IDENTIFIER-GRAMMAR split)

`checkPrivateIdentifier` (`binder.go:1323`), the binder's
`KindPrivateIdentifier` arm (`:612`): a private identifier spelled
`#constructor` is TS18012 on the identifier, in a file without parse
diagnostics. It is a binder diagnostic, so it is reported in every
declaration context, ambient included, and in plain JS (the code is on the
plain-JS list). `check_contextual_identifier` (`strict_mode.rs`) is this
port's home for the binder's per-identifier arms and is already called for
every node. The private arm runs there before the ambient return, which
gates only the `Identifier` arms (`node.Flags&ast.NodeFlagsAmbient` in
`checkContextualIdentifier`).

Correction of the record in the same function: its comment said upstream's
`yield` TS1359 arm needed `YieldContext`, which the parser did not set. That
arm is unreachable upstream. `yield` is `KindLastFutureReservedWord`
(`ast/kind_generated.go:401`), so the future-reserved arm reports first.

*Falsifier.* A TS18012 in a file with parse errors, or a missing one in a
`.d.ts`. Converted: `privateNameConstructorReserved`, `plainJSBinderErrors`.

Other splits of tsr-2zk.1166 and where they live. TS1111 is
`checkPropertyAccessExpressionOrQualifiedName`'s plain-JS arm, in
`readonly_target.rs::check_private_identifier_access` (r7-flow's). TS2804
is `checkObjectTypeForDuplicateDeclarations` (check.rs). The `#x in expr`
codes (TS2571/2406/18016/1451) are `checkInExpression`'s. TS18046 and TS7022
are receiver and inference arms. None of these is in this lane's files.

## §33 The super-call arm of `checkConstructorDeclaration` (TS2401/TS2376; CLASS-PROPERTY-INITIALIZER-CHECKS split)

`checkConstructorDeclaration` (`checker.go:2850`-`:2881`) runs this for each
constructor of a derived class-like.

* `findFirstSuperCall(body)`. With none, the constructor is TS2377's
  (`check_derived_constructor_calls_super`).
* `superCallShouldBeRootLevel` (`:2860`): not `emitStandardClassFields`
  (`useDefineForClassFields` with target ≥ ES2022), and the class has an
  instance property with an initializer or any private-named element
  (`isInstancePropertyWithInitializerOrPrivateIdentifierProperty`), or the
  constructor has a parameter-property modifier (accessibility, `readonly`,
  `override`).
* A first super call that is not a root-level expression statement of the
  body (`superCallIsRootLevelInConstructor`, through parentheses) is TS2401
  at the call. Otherwise, a body whose statements reach `super`/`this`
  (`nodeImmediatelyReferencesSuperOrThis`) before the super-call statement is
  TS2376 at the constructor.

`check_super_call_is_first` (check.rs) used to be a TS2376-only stand-in with
five departures from this:
* it had no `emitStandardClassFields` gate, which gave false TS2376 in
  `privateNameBadSuperUseDefineForClassFields`;
* it counted static initialized properties and missed private methods and
  accessors;
* it took any modifier token on a parameter as a parameter property;
* it returned on `extends null`, which upstream does not do (TS17005 is
  separate);
* it used a function-boundary scan in place of
  `nodeImmediatelyReferencesSuperOrThis`. Upstream's scan stops at a property
  declaration and at a method body, but still visits a method's computed name
  and parameters.

It also had no TS2401 arm and ran only for class declarations. The predicates
are in `class_fields.rs`. The check.rs function is the loop, now also called
from the `ClassExpression` arm.

*Falsifier.* A TS2401 on a super call that is a parenthesized root
statement, or TS2376 under `useDefineForClassFields` with target ES2022.
Converted: `privateNameBadSuperUseDefineForClassFields` (es2022, esnext),
`derivedClassSuperStatementPosition`, `derivedClassSuperProperties`.

## §34 Related information carries its file; TS6203/TS6204 on cross-file duplicates

*The file.* Every native related diagnostic is made by
`createDiagnosticForNode`, which records the node's source file. TSR's
`report(file, diagnostic)` attaches the file to the head only, so a related
diagnostic built with `Diagnostic::new` had no file. The exact oracle prints
it with an empty path and no position, so no case with checker-made related
information could be exact. Measured on base `59ceea97`: 30 cases emitted
file-less related rows (TS1498, TS1007, TS1356, TS1486). The fix:
`related_diagnostic_at` (grammar.rs) builds the related diagnostic with the
node's file attached (`Diagnostic::set_file`: the program's file name and
text, the name the head is printed under). This lane's sites use it: TS1356
(module_format.rs), TS1498 and TS1487 (grammar.rs). The others are routed:
check.rs's TS1486 and the parser's TS1007. The file image is built per call,
on error paths only, with no cache. If related diagnostics ever get hot, a
per-checker `Arc<DiagnosticFile>` keyed by file node would be the place for
one.

*TS6203/TS6204.* `reportMergeSymbolError` (`checker.go:14201`) reports
through `addDuplicateDeclarationErrorsForSymbols` →
`addDuplicateDeclarationError` (`:14232`). The error goes at each
declaration's adjusted node, and every declaration of the other symbol not at
that node is added as related information: `'{0}' was also declared here.`
first, `and here.` after, at most five, skipping one already present.
`lookupOrIssueError` finds an earlier error only while that error is still
identical to a fresh one, because `DiagnosticsCollection.Lookup` compares
related information too. So a declaration that conflicts in two merges gets
two errors, and `SortAndDeduplicateDiagnostics`' `compactAndMergeRelatedInfos`
folds them, sorting and deduplicating the related lists. That is why native
shows two `was also declared here.` and no `and here.` for `Foo` in
`duplicateIdentifierRelatedSpans1`. `merge_conflicts.rs` used to report bare
errors (its comment said `Diagnostic` had no related information, which is no
longer true). It now builds them in a pass-local `DuplicateDeclarationErrors`
(issue order, latest error per identity) and reports them at the end of
`report_merge_conflicts`.

Not here, routed: the same related information on the binder's same-file
duplicates (`binder.rs`'s `declareSymbolEx` port), on check.rs's TS2300
sites, and on TS2403's `errorNextVariableOrPropertyDeclarationMustHaveSameType`
(a 54-case oracle bucket). They can use `related_diagnostic_at`.

*Falsifier.* A cross-file duplicate whose related order or kind
(6203/6204) differs from native, or a related row without a file.
Converted (oracle subset `uplicate`): `duplicateIdentifierRelatedSpans1`–`7`,
`duplicateIdentifierRelatedSpans_moduleAugmentation`,
`letDeclarations-scopes-duplicates2`–`7`, `duplicateIdentifierEnum`.

## §35 Exact-oracle text, span and metadata fixes in this lane's reports

From `full_oracle_run rank` on `59ceea97`:

* *TS2564 message text* (5 cases, a computed name).
  `scanner.DeclarationNameToString(propName)` is the name's source text
  (`GetTextOfNode`), so `[x = 0]` prints as written. The shared
  `declaration_name_to_string` spells a computed name only through an entity
  name, giving `[(Missing)]`. `check_property_initialization` now prints
  `declaration_name_source_text` (class_fields.rs) and keeps that helper's
  answer only for the flow query's spelling and as the fallback.
* *TS2376 span* (3 cases). `GetErrorRangeForNode`'s `KindConstructor` arm
  (`scanner.go:2632`) runs from the constructor's first token to the end of
  the `constructor` keyword (or a string-literal name). The shared
  `error_span` has no such arm and gives the whole declaration.
  `constructor_error_span` (class_fields.rs) is the arm, used for TS2376.
  Routed: adding it to `error_span` itself would correct every
  constructor-anchored report (TS2377, TS2392's span-only bucket and others),
  but that helper is shared.
* *skippedOnNoEmit* (TS1216's metadata bucket, 2 cases). TS1216
  (`grammarErrorOnNodeSkippedOnNoEmit`) and TS2441/TS2818
  (`errorSkippedOnNoEmit`) are marked with the flag that
  `FilterNoEmitSemanticDiagnostics` (`compiler/program.go:1333`) reads, and
  they are dropped under `noEmit`. This program has no such filter, so
  `report_skipped_on_no_emit` (module_format.rs) drops them at the producer.
  TS2441's producer already returned under `noEmit`; TS1216 and TS2818 did
  not. Routed: the program-level filter, after which the producer drop goes.

## §36 TS1051 on a JSDoc-optional set-accessor parameter

`checkGrammarAccessor`'s `parameter.QuestionToken != nil` arm
(`grammarchecks.go`) also sees the `?` the reparser makes from a JS
accessor's `@param [x]` or `{T=}` (`makeQuestionIfOptional`). That token is
located at its tag. `check_grammar_accessor_declaration` read only a written
`?`. It now also reads `jsdoc_reparsed_function(accessor)`'s slot for the
value parameter, exactly as `check_grammar_parameter_list` reads its
reparsed `?` (ADR-0046), and reports at the tag. Converted:
`jsDeclarationsReusesExistingTypeAnnotations`.

## §37 Nothing below a `with` body is checked (needs check.rs grant (f))

`checkWithStatement` (`checker.go:4156`) checks the head expression and
reports TS2410, but never hands the statement to `checkSourceElement`. No
check of any kind runs on a `with` body upstream: grammar, names, types or
unused tracking. The references there are never resolved either, so they do
not count as uses. The check walk (`check_node`, check.rs) descended into the
body anyway. Several rules carry their own `is_inside_with_statement` guard
for this (TS1156, `r5-smallcodes2.md` §2.1), and every rule without one
reported there. `check_node` now skips the `WithStatement`'s statement child.
The per-rule guards are redundant from here on; they are kept, since they
still describe the rule's own reading. Measured on the exact oracle (filter
`ith`): `functionExpressionInWithBlock`, `withStatementErrors`,
`parserWithStatement2` and `withStatements` convert, none lost.
*Falsifier.* A native diagnostic inside a `with` body that is not from the
parser or the binder.

## §38 `error_span`'s missing `GetErrorRangeForNode` arms (needs check.rs grant (e))

`GetErrorRangeForNode` (`scanner.go:2588`) has arms that the shared
`error_span` (check.rs) lacked. Every report at these nodes therefore had the
whole node's span.

* `KindConstructor` (`:2632`, unless reparsed): from the first token,
  modifiers included, to the end of the `constructor` keyword or a
  string-literal name (`constructor_error_span`, class_fields.rs, §35).
* `KindArrowFunction`: `getErrorRangeForArrowFunction` (`:2542`), from the
  arrow's start to its end, or, when the block body's full start (the end of
  `=>`) and end are on different lines, to that first line's end
  (`arrow_function_error_span`).
* `KindFunctionExpression`: `GetNameOfDeclaration`, whose assigned-name arm
  (`GetAssignedName`, `ast/utilities.go:1486`) names an anonymous function
  after the variable, property, binding element or assignment target it is
  given to (`assigned_name_of`). With no name at all, the first token
  (`:2644`, `GetRangeOfTokenAtPosition`).
* `KindClassExpression`: its own name only, else the first token.

These change the span of every report anchored at such a node: TS2377 and
TS2392 at constructors, TS1356's related "Did you mean to mark this function
as 'async'?" (`awaitInNonAsyncFunction`), implicit-any reports at function
expressions, and others.
*Falsifier.* A span-only oracle difference at one of these four node kinds.

## §39 TS6203 on TS2403/TS2717 (subsequent declarations of another type)

`errorNextVariableOrPropertyDeclarationMustHaveSameType` (`checker.go:5949`)
reports TS2403 (variables) or TS2717 (properties) at the next declaration's
name. It adds `'{0}' was also declared here.` at the first declaration
(`createDiagnosticForNode(firstDeclaration)`, that declaration's error span).
`check_subsequent_declaration_identity` (check.rs) reported the head only. It
now attaches the related diagnostic through `related_diagnostic_at` (§34),
which carries the declaration's file. Taken as a declaration-check cluster
from the integrator's re-triage instruction; this was the 54-case
`TS6203 in TS2403` bucket of `full_oracle_run rank` on `bdd9ce10`.
*Falsifier.* A TS2403/TS2717 whose related row names a declaration other than
the symbol's first.

## §40 TS2320's TS2319 chain

`checkInheritedPropertiesAreIdentical` (`checker.go:5040`) reports TS2320 as
`NewDiagnosticChain` over a TS2319 child (`Named property '{0}' of types
'{1}' and '{2}' are not identical.`) built at the interface name. The head
takes the child's location. `check_inherited_properties_are_identical`
(heritage_conformance.rs) reported the bare head. It now builds the child
with `related_diagnostic_at` (so it carries its file, §34) and wraps it with
`Diagnostic::new_chain`. Converted (oracle):
`mergedInterfacesWithConflictingPropertyNames`, `baseTypePrivateMemberClash`,
`conflictingMemberTypesInBases`, `genericAndNonGenericInheritedSignature1`
(bucket `TS2319 in TS2320`, 14 cases).

## §41 TS2416, TS2415 and TS2430 with the relater's elaboration

`issueMemberSpecificError` (`checker.go:4472`) reports a member's TS2416 as
`NewDiagnosticChain(diags[0], Property_0_in_type_1_is_not_assignable…)`.
`diags[0]` is what `checkTypeAssignableToEx(propType, basePropType,
memberName, nil, &diags)` collected: the relater's own report (TS2322, TS2741,
…) with its chain, at the member name. The broad TS2415/TS2430 is
`checkTypeAssignableTo(typeWithThis, baseWithThis, name, broadDiag)`, the
relater's report under that head message. heritage_conformance.rs reported
all three as bare heads.

*Port.* TS2415/TS2430 go through `report_relation_failure` with the head
(`report_heritage_relation`). TS2416's child is collected by
`relation_failure_diagnostic`: it marks the diagnostics length, runs the same
reporter with no head, and drains what it added. That is the error output
container `&diags` stands for; only the first collected diagnostic is used,
as upstream uses `diags[0]`. The child gets its file attached
(`diagnostic_file_image`, §34), since nested diagnostics have no report slot.
When the reporter declines, the bare head stays, as before.

*The implements heads keep the bare head.* `Class_0_incorrectly_implements_interface_1`
(TS2420) and `Class_0_incorrectly_implements_class_1_Did_you_mean_to_extend…`
(TS2720) are `isConversionOrInterfaceImplementationMessage` heads. Those keep
themselves over a missing-property chain (`relater.go:4816`). So only TS2415
and TS2430 go through the relater (an allowlist; a first cut that exempted
only TS2420 lost five class-implements-class cases in the legacy compare).
`report_relation_failure` always lets that chain replace the head (its doc
says no caller passes such a message), so routing TS2420 through it would
turn TS2420 into TS2741. The exception is routed to the reporter's owner
(assignreport.rs); after it lands, TS2420 can take the same path.

Measured (near-full oracle vs `c91f027e2`, with §40): +19, 0 lost.
*Falsifier.* A TS2416 whose chain child differs from the relater's report at
the same member against the same base member type.

## §42 Related information on TS2394 and TS2813/TS2814

Two `checkFunctionOrConstructorSymbol` reports (`checker.go:3661`, `:3693`)
carry related information that the port dropped:

* TS2394 (`This overload signature is not compatible with its implementation
  signature.`) adds `The implementation signature is declared here.` at the
  body declaration (`check_overloads_compatible_with_implementation`,
  check.rs, a declaration check).
* TS2813/TS2814 (a class merged with a function that has a body) set the same
  list on every error: `Consider adding a 'declare' modifier to this class.`
  at each class declaration of the symbol (`class_function_merge.rs`).

Both are built with `related_diagnostic_at` (§34). Measured with oracle
filters `verload`, `unction` and `lass`: about 22 cases convert
(`callOverloads1`–`5`, `functionOverloads*`, `classOverloadForFunction`,
`augmentedTypesFunction`, …), none lost.

## §43 TS2411's property name as written, and its related declaration

`checkIndexConstraintForProperty` (`checker.go`) names the property with
`symbolToString(prop)`, i.e. `getNameOfSymbolAsWritten`
(`nodebuilderimpl.go:973`): the first declaration's name as written
(`DeclarationNameToString`), so `''`-named and `["get1"]` members print as
`'""'` and `'["get1"]'`. `index_constraint.rs` printed the symbol's bare
name. printing.rs's `symbol_name_as_written` covers identifier names only;
`property_name_as_written` (index_constraint.rs) reads any name's source
text, through `declaration_name_source_text` (§35). For a binary-expression
declaration, or one named by a computed name, that is reported elsewhere
(`propDeclaration`, `errorNode != propDeclaration`), the report adds
`'{0}' is declared here.` at the declaration. Taken as a declaration-check
cluster (index_constraint.rs has had no lane owner since dispatch).
Measured with oracle filters `ndex`, `omputedPropertyNames` and
`ymbolProperty`: +11, none lost. Not here: TS2411's span at a method
signature (bucket `TS2411 span-only MethodSignature`). `GetErrorRangeForNode`
gives a method signature its whole node (method signatures are not in its
name list), and the shared `error_span` gives the name. That is the same
arm TS7010's 10-case span bucket needs, and it belongs with grant (e).

## §44 TS2842's names as written and its TS2843

`checkUnusedRenamedBindingElements` reports TS2842 with
`DeclarationNameToString` of both names, so a property name keeps its
spelling (`"a"`, `["a"]`, `[2]`, `[sym]`). When the wrapping parameter has no
type annotation, it adds `We can only write a type for '{0}' by adding a type
for the entire parameter here.` at the parameter's end (TS2843).
`check_renamed_binding_element_in_signature` (check.rs, a declaration check)
printed the property name only when it was an identifier, and never added
the related entry. It now reads the name through
`declaration_name_source_text` (§35), records the parameter its walk already
finds, and attaches TS2843 at `Span::at(parameter.end)` with its file (§34).
Converted (oracle): `excessPropertyCheckWithSpread`,
`paramterDestrcuturingDeclaration`, `renamingDestructuredPropertyInFunctionType`
(both variants).

## §45 TS2552/TS2833's related declaration of the suggestion

`onFailedToResolveSymbol`'s spelling-suggestion arm (`checker.go:1590`)
reports `Cannot find name '{0}'. Did you mean '{1}'?` (TS2552), or TS2833 for
a namespace. When the suggested symbol has a value declaration, it adds
`'{0}' is declared here.` there. The port's reporters (check.rs's value and
type sites, namespace_not_found.rs) took only the suggestion's text from
`spelling_suggestion_for`. `spelling_suggestion_with_declaration`
(grammar.rs) makes the same single lookup
(`suggested_symbol_for_nonexistent_symbol`, with the same
global-augmentation exclusion) and also returns the related diagnostic, with
its file (§34). This was the `TS2728 in TS2552` family of `full_oracle_run
rank` (about 34 cases over five buckets). Measured near-full oracle vs
`f32a7263e`, together with §44: +42, 0 lost. Not changed: the JSX-factory
and export-specifier suggestion sites, whose messages are other codes.

## §46 TS2448/TS2449/TS2450's related declaration

`checkResolvedBlockScopedVariable` (`checker.go:1895`) adds
`'{0}' is declared here.` at the block-scoped, class or enum declaration to
every used-before-declaration report
(`AddRelatedInfo(createDiagnosticForNode(declaration, …))`). The port's
reporter (check.rs, a declaration check) emitted the head alone at both of
its report sites (the class's own computed-name/decorator arm and the
ordinary arm). `report_used_before_declaration` now builds both with the
related diagnostic, with its file (§34). This was the `TS2728 in
TS2448/TS2449/TS2450` family (about 25 cases in `full_oracle_run rank` on
`bdd9ce10`). Measured near-full oracle vs `dbb3ef7ef`: +34, 0 lost. Not here:
TS2729's related declaration (`Property used before its initialization`,
24 cases), whose reporter is readonly_target.rs (r7-flow's).

## §47 TS1192 quotes the module

`reportNonDefaultExport`'s TS1192 is `c.error(node, Module_0_has_no_default_export,
c.symbolToString(moduleSymbol))`. An external or ambient module symbol is
named with its quotes upstream (`"/.src/server"`), and with no enclosing
declaration that name is what prints. This binder names those symbols with
the bare path. `check_module_has_default_export` (check.rs, an
import-declaration check) now quotes the name of a module whose declaration
is a source file or a string-named module declaration. TS2305 is unaffected:
it prints the specifier as written (`quoted_module_name`). Converted:
`es6ImportDefaultBinding*` (5 cases, the `TS1192 message-text-only` bucket).

## §48 TS2694/TS2724 name the namespace fully qualified

`resolveEntityName`'s qualified arm reports
`Namespace '{0}' has no exported member '{1}'` (TS2694), or its TS2724
suggestion form, with `getFullyQualifiedName(namespace)` (`checker.go:14888`).
That is the resolved namespace's parents joined by `.`, with an external or
ambient module quoted (`"/.src/file".c`, and `foo.bar.baz` for an alias
`booz` of it). Both of `check_qualified_type_name_at`'s sites (check.rs)
printed the left name as written. `fully_qualified_name`
(meaning_mismatch.rs) walks the binder's parents and quotes module symbols,
which this binder names with the bare path (as §47).

*A resolution fix in the same arm.* `resolveEntityName` returns
`symbol.Flags&meaning != 0 ? symbol : resolveAlias(symbol)`, so an import
alias merged with a namespace (`import * as Lib` beside `namespace Lib`)
already has the namespace meaning and is not resolved. The port resolved
every alias. Printing the fully qualified name exposed that:
`namespaceMergedWithImportAliasNoCrash` named the module and was the one
loss in the first measurement. The alias is now resolved only without the
namespace meaning. Measured with oracle filters `lias`, `odule` and
`amespace`: +8 (`aliasBug`, `aliasErrors`, `internalAlias*`,
`moduleVisibilityTest3`, …), 0 lost.

## §49 Related declarations on TS2459/TS2460 and TS2724

`reportNonExportedMember` (`checker.go:14908`) adds `'{0}' is declared here.`
then `and here.` at every declaration of the module's local symbol to TS2459
(`declares … locally, but it is not exported`) and TS2460 (`… exported as`).
`errorNoModuleMemberSymbol`'s TS2724 suggestion adds `'{0}' is declared
here.` at the suggested export's value declaration (`:14898`). The
import-specifier reporter in symbols.rs (an import-declaration check) built
the heads alone. It now attaches them through `related_diagnostic_at` (§34);
the suggestion's symbol is `get_export_of_module(target, suggestion)`.
Converted (near-full oracle vs `02301e3b`): `importNonExportedMember`–`3`,
`missingMemberErrorHasShortPath`, `exportSpellingSuggestion`; +6, 0 lost.

## §51 TS1046 on the first token

`checkGrammarTopLevelElementForRequiredDeclareModifier` reports TS1046 with
`grammarErrorOnFirstToken`, i.e. the statement's first token (`var`,
`namespace`, `function`). `check_top_level_declare_modifiers` (check.rs, a
grammar check) reported the whole statement. It now takes
`first_token_span`. Converted: `missingRequiredDeclare.d.ts`,
`erasableSyntaxOnlyDeclaration` and seven `parser*.d.ts` cases (the
`TS1046 span-only` buckets).

## §52 TS1107/TS1108 on the `return` keyword

`checkReturnStatement` reports TS1108 (and TS1107 in a class static block)
with `grammarErrorOnFirstToken`, i.e. the `return` keyword. Its doc comment
said so, but `check_return_container` (check.rs) reported through
`report_grammar_at`, which takes the whole statement span. It now takes
`first_token_span`. Converted: `fileWithNextLine3`,
`multiLinePropertyAccessAndArrowFunctionIndent1`,
`parserErrorRecovery_VariableList1`, `parserReturnStatement1`/`2` and
`parserNotRegex1` (the `TS1108 span-only` buckets).

## §53 An async or generator IIFE ends the same-scope walk

`isSameScopeDescendentOf` (`checker.go:2090`) stops at a function-like that
is not an immediately invoked one, **or** whose
`GetFunctionFlags(n) & FunctionFlagsAsyncGenerator != 0`.
`FunctionFlagsAsyncGenerator` is the two bits, Async and Generator, so the
test is either of them. A use inside an async arrow or a generator IIFE in a
variable's own initializer is therefore not "immediately used"
(`const promise = (async () => { …; promise })()`). `is_async_generator`
(check.rs, a declaration check) required both bits, and only on a function
expression, so those uses were TS2448. It now reads either bit on any
function-like (arrow: `async` only). Converted:
`blockScopedVariablesUseBeforeDef`.

## §56 TS1106 reads `NodeFlagsAwaitContext`; the stale "parser drops `await`" comments

*Renumbered:* this section was first committed as a second §53 (`cee33e03`),
beside the IIFE section above; the number is now its own.

`checkGrammarForInOrForOfStatement`'s `async` arm (`grammarchecks.go:1251`)
reports `for (async of xs)` on a statement without `NodeFlagsAwaitContext`.
`check_grammar_for_of_async` (grammar.rs) dated from before the parser set
the flag and read the enclosing function's `async` modifier instead. That
stand-in disagrees with native in two places, both await contexts: a class
static block (`parseClassStaticBlockBody` sets the await context, so native is
silent; the stand-in reported TS1106) and a top-level statement that
`reparseTopLevelAwait` reparses. The arm now reads the flag r7-parser
`1321f0aa` stamps (§9). Test: `tests/for_of_async_await_context.rs`, checked
against `target/tsgo-pinned`.

The same commit corrects three doc comments that still described the parser
before r7-parser `1321f0aa` and its `reparseTopLevelAwait` port:
`check_top_level_await` (module_format.rs: "the ambiguous spellings this
parser leaves as identifiers"), `check_contextual_identifier`
(strict_mode.rs: "the arm that is not ported … declared and never set") and
the grammar.rs arm above. A fourth one is outside this lane:
`check_await_as_binding_name` (check.rs) still says the flag is unset and
walks for an enclosing `async`. That was reported to the integration thread.

*Falsifier.* A TS1106 in a static block, or a missing one in a non-async
function, in a corpus case.

## §54 TS2373's parameter as written; TS18033 through the relater

*TS2373/TS2372.* `checkResolvedBlockScopedVariable`'s parameter arm reports
`Parameter '{0}' cannot reference identifier '{1}' declared after it.` with
`DeclarationNameToString(associatedDeclaration.Name())`. That is the name's
source text, so a binding pattern prints as written, computed keys and
defaults included (`{ [a() ?? "d"]: c = "" }`). class_fields.rs's
`binding_name_text` rebuilt the pattern from its element names (`{ c }`).
It now takes `declaration_name_source_text` (§35) and keeps the rebuild only
as the no-source fallback. Converted (CLI vs `target/tsgo-pinned`):
`nullishCoalescingOperatorInParameterBindingPattern.2`,
`classWithStaticFieldInParameterBindingPattern.2` (4 oracle rows, the
`TS2373 message-text-only` buckets).

*TS18033.* `computeConstantEnumMemberValue`'s default arm is
`checkTypeAssignableTo(checkExpression(initializer), numberType, initializer,
Type_0_is_not_assignable_to_type_1_as_required_for_computed_enum_member_values)`.
That is the relater's `reportRelationError` under the head, so a literal
source is generalized against `number` (`typeCouldHaveTopLevelSingletonTypes`
is false), and the elaboration chain follows. enum_initializer.rs printed
`type_to_string(source)`. That gave `'"bar"'` for `` `${BAR}`! ``, where
native prints `'string'`, and it dropped the chain (`TS2322 in TS18033`,
`TS2692 in TS18033`). The check now goes through `report_relation_failure`
with the head, as heritage_conformance.rs's TS2415/TS2430 do (§41). A
declined report keeps the bare head.

*Falsifier.* A TS18033 whose type text or chain differs from the relater's
TS2322 for the same pair. That would be the relater's divergence, not this
call's.

## §55 TS18027, TS2398, TS2500, and TS2416's member name as written

Four small arms of declaration checks. Each had no emitter or printed the
wrong text. All were checked against `target/tsgo-pinned`
(`tests/declaration_collisions_grammar.rs`).

- **TS18027** (`recordPotentialCollisionWithWeakMapSetInGeneratedCode` /
  `checkWeakMapSetCollision`, `checker.go:10539`): at ES2021 or below, a
  declaration named `WeakMap`/`WeakSet` whose enclosing block-scope container
  holds a class element named with a private identifier. Native marks every
  enclosing container from `checkPropertyDeclaration`/method/accessor
  (`setNodeLinksForPrivateIdentifierScope`) and defers the test. As with
  TS2818's `Reflect` (`check_reflect_collision`), the deferred mark is read
  off the tree. The container is marked exactly when such an element lies
  strictly inside it. `checkCollisionsForDeclarationName` records this
  collision just before the `Reflect` one, so `check_reflect_collision` calls
  it first. `errorSkippedOnNoEmit`, so a `--noEmit` run is silent. Converted:
  `privateNameWeakMapCollision`.
- **TS2398** (`checkParameter`, `checker.go:2674`): a parameter property
  named `constructor` in a constructor, a `c.error` at the name. It runs from
  grammar.rs's per-node `check_parser_lane_statement` hook. Its TS2369
  sibling stays in check.rs. Converted: `parameterPropertyInConstructor3`.
- **TS2500** (`checkClassLikeDeclaration`'s implements loop,
  `checker.go:4369`): an implements entry that is not an entity name
  expression, or is an optional chain (`implements A?.B`). It is reported
  before any of heritage_conformance.rs's declines, as native reports it
  before the relation. Converted: `classExtendingOptionalChain`.
- **TS2416's `{0}`** is `symbolToString(declaredProp)`, whose
  `getNameOfSymbolAsWritten` is `DeclarationNameToString(name)`
  (`nodebuilderimpl.go:1002`). A string-literal or numeric-literal member
  name prints as written (`'2.0'` with its quotes). `issue_member_specific_error`
  printed the cooked text. Converted: `subtypingWithObjectMembers`.

*Falsifier.* A TS18027 where native's marks differ from the subtree read,
i.e. a private-named element that native never checks (an unchecked
region). That would show as an extra TS18027.

## §57 TS2420/TS2720 with the relater's elaboration; TS2300 at an import's name

*TS2420/TS2720.* §41 sent TS2415 and TS2430 through `report_relation_failure`
and kept both implements heads bare. `Class_0_incorrectly_implements_interface_1`
and `…implements_class_1_Did_you_mean_to_extend…` are
`isConversionOrInterfaceImplementationMessage` heads, which keep themselves
over a missing-property chain (`relater.go:4816`). The reporter did not make
that exception then. r7-reports `ec17ba88` added it (assignreport.rs
`is_conversion_or_interface_implementation_message`), so
`report_heritage_relation` now sends every head through the relater, as
native's `checkTypeAssignableTo(typeWithThis, baseWithThis, name, broadDiag)`
does. Checked with the CLI against `target/tsgo-pinned`: `classImplementsClass2`/`4`,
`genericConstraint2`, `inheritance1`, `implementPublicPropertyAsPrivate`,
`classExtendsInterfaceThatExtendsClassWithPrivates1`,
`classWithMultipleBaseClasses` and `declareClassInterfaceImplementation` now
match. That is the `TS2741/TS2739/TS2740/TS2325/TS2442/TS2329 in TS2420` and
`… in TS2720` buckets of `full_oracle_run rank` (about 39 sole-code cases).

*TS2300 at an import's name.* `addDuplicateDeclarationError` reports at
`getAdjustedNodeForError(node)`, i.e. `GetNameOfDeclaration`. That covers an
import-equals declaration, an import clause, an import specifier and an export
specifier, which `error_span`'s declaration list does not name.
`duplicate_declaration_span` (merge_conflicts.rs) now takes their names. So
`export import VNode = react.ReactNode` merged across files with
`type VNode` reports at `VNode` (`checkerInitializationCrash`).

Not here: `umdGlobalConflict`'s extra TS2300. Native adds a file's
`GlobalExports` with first-in-wins and never merges them
(`checker.go:1321`). This binder reports the second `export as namespace
Alpha` while declaring into its program-wide `global_exports` table
(binder.rs `Destination::GlobalExports`). That is the binder's, routed.

## §58 TS6205's range and TS1184's first token on an object-literal method

*TS6205.* `checkUnusedTypeParameters` reports `All type parameters are
unused.` at `rangeOfTypeParameters` (`utilities.go:1554`):
`[list.Pos() - 1, SkipTrivia(text, list.End()) + 1)`. This tree keeps no
`NodeList` positions. unused.rs took one byte before the first parameter to
its end, which is `<T` for `<T, V>` and wrong for every JS `@template` list.
`range_of_type_parameters` recovers the list's positions:

- A written list starts right after its `<`. That `<` is found by stepping
  back over whitespace and block comments from the first parameter. `End()`
  is after the last parameter, or after a trailing comma.
- A JS list that `gatherTypeParameters` (`reparser.go:293`) hosts from
  `@template` tags runs from the first template tag's `Pos()` to the last
  one's `End()`. Those are the tags' own spans, so the range takes in the
  `*` of the closing ` */` or of the next line, as native's does.

Checked with `--pretty true` against `target/tsgo-pinned`: `< T, V >`,
`<A,B,>`, `</*x*/A, B /*y*/ >`, and `@template` lists across tags, with a
tag comment, with a constraint, and interleaved with `@param`.
Converted: `unusedTypeParameters_templateTag2`.

*TS1184.* `checkGrammarMethod` reports modifiers on an object-literal
method (other than a lone `async`) with `grammarErrorOnFirstToken(node, …)`,
i.e. the whole first modifier. grammar.rs's postfix-token walk reported a
one-byte span there. Correction of the record: it now goes through
`grammar_error_on_first_token`. Converted: `objectLiteralMemberWithModifiers1`.

## §59 TS2737 and TS2439

Two arms with no emitter. Both are reached from grammar.rs's per-node
`check_parser_lane_statement` hook. Both checked with `--pretty true`
against `target/tsgo-pinned`.

- **TS2737** (`checkGrammarBigIntLiteral`, `grammarchecks.go:2105`, from
  `checkExpression`'s `KindBigIntLiteral` arm): below ES2020, a bigint
  literal outside a literal type node (`1n`, or `-1n` as one) and outside an
  ambient context (`has_ambient_flag`) is a grammar error. Converted:
  `bigintAmbientMinimal`, `bigintWithoutLib` (es2015 variants).
  Correction of the record: the first cut also reported a bigint literal
  that is a declaration's name (`enum E { 0n = 0 }`, `enumWithBigint`).
  `checkExpression` never reaches a name, so the check now skips the
  parent's `name` slot. Measured on the exact oracle at `e50e75c2`: one
  extra line in a case that was not EXACT.
- **TS2439** (`checkExternalImportOrExportDeclaration`, `checker.go:5348`):
  an import or export in a string-named ambient module's block, whose module
  name is relative (`tspath.IsExternalModuleNameRelative`), is reported at
  the declaration unless the ambient module is an external module
  augmentation (`isTopLevelInExternalModuleAugmentation`). It sits beside
  the same function's TS1141 arm. The TS1147/TS1198 arm between them is
  check.rs's; the two exclude each other. Converted:
  `ambientExternalModuleWithRelativeExternalImportDeclaration`.

*Not here.* Native's caller checks the import binding (and so resolves the
module) only when `checkExternalImportOrExportDeclaration` returns true. A
TS2439 declaration therefore never draws TS2307. This port's module report
does not read that answer, so `import * as b from "../y"` in `declare
module "A"` still gets an extra TS2307 here. That was true before this
change too. Routed to the import-declaration check's owner.

## §60 TS2564 skips a property whose annotation is circular

`checkPropertyInitialization` skips a property whose type has
`TypeFlagsAnyOrUnknown`, and `errorType` has it. A property annotated with
itself (`class C1 { x: C1["x"]; }`) fails `popTypeResolution` in
`getTypeOfVariableOrParameterOrProperty`, which reports TS2502 and answers
`errorType`. This port reports the same TS2502 (symbols.rs), but its outer
resolution keeps the indexed access's answer (a gap), so
`check_property_initialization` (granted) went on to TS2564. It now also
skips a declaration in `circularity_reported`, the set symbols.rs records at
exactly that report. Converted: `circularIndexedAccessErrors`'s extra TS2564.
The case keeps its TS2589 and TS2536 gaps, which are not this lane's.

## §62 TS1335/TS1331/TS1332 for a JSDoc `@type {unique symbol}`

`checkGrammarTypeOperatorNode` judges `unique symbol` by the type's parent,
after `WalkUpParenthesizedTypes`. In JS, `reparseHosted`'s `KindJSDocTypeTag`
arm (`parser/reparser.go:344`) moves a `@type` onto its declaration: the
first untyped declaration of a documented variable statement, a documented
variable declaration, or a property declaration. That declaration is then
the parent. This tree keeps the JSDoc type under its `JSDocTypeExpression`,
so `/** @type {unique symbol} */ const x = Symbol()` drew TS1335
(`uniqueSymbolJs`, `uniqueSymbolJs2`). `jsdoc_type_reparse_host` maps the
outermost type back to that declaration. It starts at the comment's host
(`jsdoc_hosts`) and accepts a declaration only when its JSDoc type reader
(`jsdoc_type_annotation`, `jsdoc_self_hosted_type`) answers this very node.
The variable/property arms then run as written. The property arm reads
`hasReadonlyModifier`, i.e. `ast.HasModifier`, so a JS `@readonly` counts
(`has_effective_modifier`). Checked with `--pretty true` against
`target/tsgo-pinned` at es2015 and es2022: `let`/`const`, a multi-declaration
statement, a parenthesized type, static/instance/`@readonly` class fields, a
`@param` (still TS1335) and a type-literal member (TS1330).

## §63 TS1326 on `import<T>` as an expression

`checkGrammarExpressionWithTypeArguments` (`grammarchecks.go:885`), reached
from `checkExpressionWithTypeArguments` and `checkGrammarHeritageClause`,
reports an `ExpressionWithTypeArguments` whose expression is the `import`
keyword and which has a type-argument list. The report is TS1326 on the
node, a grammar error. It had no emitter. It now runs from grammar.rs's
per-node hook. Converted: `importWithTypeArguments`. Not here:
`import<T>("./0")`, a call. Its TS1326 is `checkGrammarImportCallExpression`'s,
import_call_grammar.rs, along with `import<>` (an empty list, which this AST
cannot tell from none). Routed.

## §61 TS2702/TS2713, a type used as a namespace (needs namespace_not_found.rs grant)

`onFailedToResolveSymbol`'s `checkAndReportErrorForUsingTypeAsNamespace`
arm (`checker.go:1608`) applies to a name resolved with
`SymbolFlagsNamespace` that resolves, with aliases followed, as `Type &^
Namespace`. It is TS2713 on the whole qualified name when the left side's
declared type has the right side as a property (`C2.Red`, did you mean
`C2["Red"]`). Otherwise it is TS2702 on the name. namespace_not_found.rs's
`report_cannot_find_namespace` returned silently in that case; its doc said
the arm was unported. The arm is meaning_mismatch.rs's
`report_type_used_as_namespace`, the cascade module's fifth arm. The
early return in namespace_not_found.rs (integration owner's file) calls it.
Checked with the CLI against `target/tsgo-pinned`:
`invalidUseOfTypeAsNamespace`, `invalidImportAliasIdentifiers`,
`decoratorMetadataWithImportDeclarationNameCollision7` now match.
`errorForUsingPropertyOfTypeAsType03` gains its TS2702/TS2713 lines (it keeps
a TS2749 gap). `strictModeReservedWordInClassDeclaration`'s
`implements public.private.B` is a property-access heritage name, which
check.rs does not resolve through this reporter.

*One decline.* `@callback NS.Nested.Inner` (or `@typedef`) declares
namespace `NS` upstream (`parseJSDocTypeNameWithNamespace`, then
`bindJSDocTypeAlias`). This parser keeps only `NS` as the tag's name, and
the alias is filed under it as a type. So the arm would report TS2702 on a
later `NS.Nested.Inner` that native resolves (`callbackTagNamespace`, a
legacy-suite loss in the first gate). An alias whose written name continues
with `.` stops the cascade unreported, as before the arm existed. Routed to
r7-parser (the namespaced JSDoc name).
## §64 TS2417 with the relater's elaboration (needs check.rs grant)

`checkClassLikeDeclaration` reports the static side (`checker.go:4337`) with
`checkTypeAssignableTo(staticType, getTypeWithoutSignatures(staticBaseType),
name, Class_static_side_0_incorrectly_extends_base_class_static_side_1)`.
That is the relater's report under the head, with its chain. check.rs's
`check_static_side_assignability` decides the failure member by member (its
own approximation, unchanged here) and then reported the bare head. It now
reports through `report_relation_failure` with the head, as
heritage_conformance.rs's TS2415/TS2430/TS2420 do (§41, §57). A declined
report keeps the bare head. Checked with the CLI against
`target/tsgo-pinned`: nine of the thirteen TS2417 cases of the exact oracle
now match (`derivedClassOverridesPrivates`,
`derivedClassWithPrivateStaticShadowingPublicStatic`,
`inheritanceStatic*Overriding*`, `inheritanceStaticMembersIncompatible`,
`inheritedModuleMembersForClodule`, `overridingPrivateStaticMembers`).
`clodulesDerivedClasses` keeps a relater chain difference (`typeof
Path.Utils` names). `classExtendsNull2` keeps a missing TS2417, which the
approximation declines (`extends null`).
## §65 TS8023, an `@augments`/`@extends` name that does not match the `extends` clause (needs check.rs grant)

`checkGrammarClassDeclarationHeritageClauses`'s `extends` arm
(`grammarchecks.go:915`) walks the class's comments after the
single-class test. For each `JSDocAugmentsTag`, when the tag's class name's
identifier (`getIdentifierFromEntityNameExpression`: the name, or a property
access's right side) differs from the written `extends` type's, it reports
TS8023 at the tag's name with the tag name, both identifiers, and returns.
check.rs's `check_grammar_heritage_clauses` had no such arm. It now calls
`check_grammar_jsdoc_augments_name`. A tag written without a class name
holds native's missing identifier: empty text, zero width where the tag name
ends (the JSDoc parser's TS1003 is at the same place). Converted, checked
with the CLI against `target/tsgo-pinned`: `extendsTagEmit`,
`jsdocAugmentsMissingType`, `jsdocAugments_nameMismatch`.
## §66 Where the lane stands (r7-grammar (2), 2026-10-12)

*Landed through box/r7-grammar* (§53–§63, merged on main through batch
63). Exact oracle on this thread's gates: 10574 at the start, 10621 at
`0eb482de`.

*Gated on box/r7-grammar-held, waiting for grants* (`d677f9168`: +76 exact
vs a freeze of `1fef27b3`, 0 lost; legacy +18 cases, 0 lost):

- the old held stack (§25, §26, §28, §31, §37, §38): check.rs grants (a)–(f);
- §61 TS2702/TS2713: `namespace_not_found.rs::report_cannot_find_namespace`;
- §64 TS2417's chain: check.rs `check_static_side_assignability`;
- §65 TS8023: check.rs `check_grammar_heritage_clauses`.

*Routed, not this lane's* (each with its evidence in the section that
found it):

- the relater's chains under TS2416/TS2420/TS2430 (TS2326 signature
  chains, TS2634 index signatures, a private member's declaring class);
- JSDoc type references skipped by `check_type_reference_name`
  (`names_in_jsdoc`, ~30 TS2749/TS2304 cases);
- JSDoc `@import` tags never reaching `check_import_attributes` or TS1141;
- TS1099/TS1098/TS1025/TS1013: the AST keeps no empty-but-present list or
  trailing comma;
- binder: TS2300's display name as written, TS2528's related information,
  the UMD `GlobalExports` first-in-wins (§57);
- members.rs: `get_property_of_type` misses an inherited member of a generic
  interface reference (hides TS2320);
- `import { X }` against `export =` of a const merged with a namespace
  resolves value-only (`mergedDeclarations7`);
- `this`-argument images not read by the relater (TS2415/TS2430 extras on
  `this`-typed members: `thisTypeInFunctions3`, `contextualThisType`);
- import_call_grammar.rs: TS1326 on `import<T>(…)`;
- TS2439's caller still reporting TS2307 (§59).

*Refused here.* Removing heritage_conformance.rs's default-library base
decline (`has_default_library_declaration`) was measured with the CLI on
`subclassUint8Array` and `classExtendingBuiltinType`. It reports a false
TS2415 on both (the lib-merged member tables are incomplete). It stays.

## §67 TS1539, a bigint literal as a property name

`A 'bigint' literal cannot be used as a property name.` has two native
sites, both `c.error`s that stand in a file with parse diagnostics:

- `checkVariableLikeDeclaration` (`checker.go:5890`), for a property
  declaration or property signature named `4n`;
- `checkGrammarObjectLiteralExpression`'s property-assignment arm
  (`grammarchecks.go:1111`, `addErrorOrSuggestion(true, …)`), for `{ 1n: x }`.

It had no emitter. `check_big_int_property_names` reports both from
grammar.rs's per-node hook. A binding element's `{ 0n: f }` is neither
(native reports TS2538 there, not this). Checked with the CLI against
`target/tsgo-pinned` (object literal, class field, type-literal member).
`bigintPropertyName`'s nine TS1539 lines are the oracle's measure. The case
keeps other gaps.

## §68 TS7061, members in a mapped type

`checkMappedType` starts with `checkGrammarMappedType`
(`grammarchecks.go:120`). A mapped type written with members
(`{ [P in K]: V; model: string }`) is TS7061 on the first member, a grammar
error. The node's `members` are already parsed; the check had no emitter.
It runs from grammar.rs's per-node hook. Checked against `target/tsgo-pinned`
for a property member (span equal). A method member also gets TS7061, but
two parser differences remain there, both routed to r7-parser. This parser
makes the member a method declaration where native has a method signature,
so its error span is the name rather than the whole member. It also draws
an extra TS2391 that native's `checkMappedType` (which never checks the
members) does not report.

## §69 Routed-back fixes in other lanes' files (default small-hunk grant)

Each of these is one native root cause with its consumer hunk. They were
granted by default for small, atomic hunks (integrator, 2026-10-12). The
file and function are named for each owner.

- **TS1359 reads `AwaitContext` only.** `binder.go:1312`'s arm is
  strict_mode.rs's `check_contextual_identifier` (§9). check.rs's
  `check_await_as_binding_name` predated the flag and walked for an
  enclosing `async`. That gave an extra TS1359 on `function g(await) {}`
  nested in an async function, where native's parser leaves the await
  context. *Removed* (check.rs). The flag it now relies on was missing on
  one name: `parseFunctionExpression` parses an async/generator expression's
  name in its own await/yield context (`doInContext(…, true, …)`,
  `parser.go:5694`). tsr-parser `expression.rs::parse_function_expression`
  now does the same, so `var v = async function await() {}` keeps TS1359
  (`asyncFunctionDeclaration12_es2017`).
- **TS1486 carries its file in JS.** check.rs's JS decorator-placement arm
  built `Decorator used before 'export' here` without a file. It now takes
  `related_diagnostic_at` (§34;
  `esDecorators-classDeclaration-exportModifier`).
- *Refused: no TS2307 under TS2439.* `checkImportDeclaration` resolves
  the module only when `checkExternalImportOrExportDeclaration` returns true,
  and TS2439's arm returns false. A first cut made check.rs's
  `check_module_specifier` return there too. The exact oracle lost
  `ambientExternalModuleWithRelativeExternalImportDeclaration`: native still
  reports TS2307 when the alias is *used* (`m2.c` resolves the alias and
  with it the module). This port reports the module at the declaration, not
  at a use, so the skip would only be right for an unused alias. Withdrawn;
  §59's extra TS2307 on an unused alias stays.
- **TS1326 on `import<T>(…)`.** `checkImportCallExpression` reports a written
  type-argument list (`grammarchecks.go:2176`) before its argument arms.
  The arm is now in import_call.rs's `check_import_call_specifier`. An empty
  `import<>` is found through `written_list`, as is §63's instantiation-
  expression arm. An import call never runs `checkGrammarTypeArguments`
  (`checker.go:7773`), so grammar.rs's `check_grammar_type_arguments` skips
  it, and it reports no TS1099. Converted (CLI): `importCallExpressionWithTypeArgument`.

## §70 JSDoc `@import` tags: TS1141 and the import-attribute checks

The reparser makes each `@import` tag a `JSImportDeclaration` among its
host's statements (`reparseUnhosted`, `parser/reparser.go:119`), and
`checkImportDeclaration` checks it like any import. That covers TS1141 from
`checkExternalImportOrExportDeclaration` and `checkImportAttributes`, with
the declaration exclusively type-only, so TS2857 under `esnext` and TS2823
below it. This tree keeps the tag in its comment, which the check walk does
not descend into. `check_jsdoc_import_tags` (import_attributes.rs) runs both
checks from the host's visit (check.rs's JS arm, beside the reparsed-type
walk). `check_import_attributes` accepts the tag. The module-element-context
test does not apply to it: a comment has no parent edge here. Also changed:
import_attribute_checks.rs's `check_import_attributes_assignable` reads the
tag's attributes, and grammar.rs's TS1141 reader admits a tag as its
declaration. Expected to convert (oracle): `importTag13`, `importTag15`
(both variants), `importDeferJsdoc`. `importTag14` keeps a TS2306 that
belongs to module resolution.

## §71 TS2708 for an exported `import a = x.c` that does not resolve

`checkImportEqualsDeclaration` calls `markLinkedReferences(node,
ReferenceHintExportImportEquals)`. For an exported alias that is
`markExportAsReferenced` (`checker.go:28845`). A target that does not resolve
is `unknownSymbol`, a `Property`, so the value arm marks the alias, and
`markIdentifierAliasReferenced` resolves the leftmost identifier as a value
(`getResolvedSymbol`). When that fails, `onFailedToResolveSymbol` runs; for
a name that is a namespace it reports TS2708, *Cannot use namespace 'x' as a
value*. meaning_mismatch.rs's `check_exported_import_equals_reference` runs
that path for a qualified module reference. It sits behind the same gates:
no `verbatimModuleSyntax`, not ambient, and a legal module-element context.
It is called from check.rs's import-equals arm. A name that resolves as
nothing keeps only its TS2503. Checked with the CLI against
`target/tsgo-pinned`: `importDeclWithClassModifiers` and the
`declare export import` control `importDeclWithDeclareModifier` (silent,
ambient).

## §72 JSDoc type names are checked; the intended JSDoc names are not

check.rs's `check_type_reference_name` declined every name inside a JSDoc
comment (`names_in_jsdoc`, r6-names §6). Lifting the decline then measured
+4 and 6 lost. Re-measured on main batch 72 with the native exclusion that
was missing: `getTypeFromTypeReference` asks
`getIntendedTypeFromJSDocTypeReference` (`checker.go:23020`) first. For a
JSDoc reference named `String`, `Number`, `BigInt`, `Boolean`, `Void`,
`Undefined`, `Null` or `Function`/`function`, for `array`/`promise` with no
type arguments and `noImplicitAny` off, and for `Object` with two type
arguments or `noImplicitAny` off, it answers without resolving the name. So
no TS2304/TS2583/TS2552 is reported there (`checkJsdocTypeTag8`'s `BigInt`,
`jsdocFunctionType`'s `function`). `is_intended_jsdoc_type_reference_name`
(name_slots.rs, the integration owner's file, a small hunk) is that
exclusion. The decline is otherwise gone (check.rs, one condition). Exact
oracle against a freeze of `8da3d42d`: +24, 0 lost. The new cases are
JSDoc TS2749/TS2304 reports (`jsdocTypeReferenceToValue`, `enumTag`,
`jsEnumTagOnObjectFrozen`, `jsdocTypeReferenceExports`,
`exportedEnumTypeAndValue`, `jsdocTypeReferenceToImport`,
`jsdocResolveNameFailureInTypedef`, …).
*Falsifier.* A TS2304/TS2749 inside a JSDoc comment that native does not
report, from a JSDoc-specific resolution this port lacks (typedef scopes,
`import()` types).

## §73 TS2309 from `checkExternalModuleExports`

`An export assignment cannot be used in a module with other exported
elements.` is the `export =` half of `checkExternalModuleExports`
(`checker.go:5693`). Native runs it once per module symbol
(`exportsChecked`): for an external or CommonJS source file after its check
(`checkSourceFile`), and for an export assignment's container (the source
file, or the module declaration around the block, `checkExportAssignment`).
It reports at the `export =` alias's declaration (or its value declaration:
a JS `module.exports =`) when the module also exports a value
(`hasExportedMembersOfKind` over the alias-resolved flags of every other
export) or when the alias is a namespace shadowing one that exports types or
namespaces (`hasShadowedNamespace`). It is not reported in an external
module augmentation.

check.rs's export-assignment arm approximated this syntactically: a
top-level `export =` beside an exported class, function, variable, enum or
`export {…}`, checked only after the TS1203 module-format arm returned. So
`export function f() {}` + `export = f` under ES modules (TS1203 and
TS2309), an ambient module's `export = y` beside `export interface x`, and
every JS `module.exports =` were missed. merge_conflicts.rs's
`check_external_module_export_equals` is the port (checker.rs gains its
`export_equals_checked` set). check.rs calls it from both native sites and
drops the approximation. An exported `import a = x.c` whose target does not
resolve counts as a value: native's `unknownSymbol` is a `Property`. That is
meaning_mismatch.rs's `unresolved_qualified_import_equals`, shared with §71.
Checked with the CLI against `target/tsgo-pinned`: `es6ExportEquals`,
`importDeclWithExportModifierAndExportAssignment`, `incompatibleExports1`,
`jsExportAssignmentNonMutableLocation`, `moduleExportAssignment`,
`exportAssignmentWithExports`, `exportEqualsDefaultProperty`.

## §74 TS1361/TS1362 carry `'{0}' was imported here.` / `exported here.`

`resolveNameHelper`'s type-only arm (`checker.go:1860`) reports TS1361/TS1362
through `addTypeOnlyDeclarationRelatedInfo` (`:2168`). That adds `'{0}' was
imported here.` at the type-only declaration, or `exported here.` when the
declaration is an export specifier, export declaration or namespace export.
check.rs's `report_type_only_alias_used_as_value` built the head alone. It
now attaches the related diagnostic, with its file (§34), at the declaration
isolated_alias.rs's `type_only_alias_declaration_node` answers. That is
`getTypeOnlyAliasDeclaration`'s node, which this change makes `pub(crate)`
(a one-word hunk in that file). Checked with `--pretty true` against
`target/tsgo-pinned`: `chained`, `enums`. These are the `TS1376 in TS1361` and
`TS1377 in TS1362` buckets (about 23 cases).

## §75 A method signature's error span is the whole member

`GetErrorRangeForNode` (`scanner.go:2588`) names a function declaration or
method declaration by its name (`:2598`), and a fixed list of declaration
kinds by theirs (`:2604`). A method signature is in neither, so a
diagnostic at one spans the whole member. check.rs's `error_span` read every
declaration's name through `declaration_name_of`, so TS2411 on an
interface's `bar(): any;` underlined `bar`. §43 left this for the error_span
grant; it now has its own arm (check.rs `error_span`, one match arm). Checked
with `--pretty true` against `target/tsgo-pinned`:
`interfaceMemberValidation`, `interfacedeclWithIndexerErrors`,
`propertiesAndIndexers` (TS2411) and `reservedWords3`,
`MemberFunctionDeclaration5_es6`, `parserEqualsGreaterThanAfterFunction1`.

## §76 A source file's module symbol prints as `"` + its path without its extension + `"`

`symbolToString` of a source file's symbol is its name, which the binder
makes `"` + `tspath.RemoveFileExtension(fileName)` + `"`. That function strips
`.d.ts`/`.d.mts`/`.d.cts` whole. This binder names the symbol with the bare
path minus only the last extension, so §47's TS1192 and §48's
fully-qualified names printed `"/.src/other.d"` for a declaration file
(`esmModeDeclarationFileWithExportAssignment`). `module_symbol_name`
(meaning_mismatch.rs) is the one reader for both. It takes a source file's
name from the host's file path through `remove_file_extension`, and quotes
a `declare module "x"`'s name.

## §77 TS1192's `'export *' does not re-export a default.`

`reportNonDefaultExport` (`checker.go:14591`) relates the first
`export * from "m"` of the module (its `__export` declarations) whose target
module has a `default` export. That is TS1195, at the export declaration.
check.rs's `check_module_has_default_export` built the head alone. It now
adds the related diagnostic, with its file (§34). Checked with `--pretty
true` against `target/tsgo-pinned`: `exportStar` (es2015).

## §78 TS2449/TS2450 on an exported class or enum

`resolveNameHelper` checks block-scoped use with the export symbol of an
exported local (`getExportSymbolOfValueSymbolIfExported(result)`). The local
of `export class C`, `export default class E` or a namespace's `export class
D` is only `ExportValue`; its export symbol carries `Class`. check.rs's
`check_used_before_its_declaration` read the local's flags, found no arm,
and was silent on every exported class or enum used before its declaration
(`es5ExportDefaultClassDeclaration3` es2015, `parserRealSource10`). §964's
`export const bar = bar` special case had covered variables only. It now
takes isolated_alias.rs's `export_symbol_of_value_symbol_if_exported`
(made `pub(crate)`) first. Checked with the CLI against `target/tsgo-pinned`
on module, namespace and default exports. The used-before-declaration
cases above are unchanged.

## §79 Decorators of a node that cannot be decorated are not checked

`checkDecorators` (`checker.go:6025`) returns first when
`!CanHaveDecorators(node) || !HasDecorators(node) ||
!NodeCanBeDecorated(legacyDecorators, node, …)`, so no decorator of such a
node is checked: not its expression and not its names. A parameter under
ES decorators is the common case. The grammar error for the misplaced
decorator is `checkGrammarModifiers`', reported from the owner. check.rs's
walk visited every decorator, so `constructor(@dec(C5) a: any)` drew TS2449
(`useBeforeDeclaration_classDecorators.2`, `experimentalDecorators: false`).
The walk now skips a Decorator child whose owner fails grammar.rs's
`node_can_be_decorated` (check.rs `check_node_worker`, the child loop; the
predicate runs only for a node with a decorator child). Checked with the
CLI against `target/tsgo-pinned`: both variants of that case and the
parameter-decorator cases.

## §80 TS2300 on duplicate members names the member as written

`reportDuplicateMemberErrors` (`checker.go:3213`) reports
`c.symbolToString(symbol)`. For a member that is not late-bound that is
`getNameOfSymbolAsWritten`: `DeclarationNameToString` of the first
declaration's name, so `0b11`, `"artist"`, `0.0` and `["a"]` print as
written. check.rs's `check_object_type_for_duplicate_declarations` printed
the binder's canonical name (`3`, `artist`, `0`, `a`). It now prints the
first declaration's name source text (`declaration_name_source_text`, §35),
the canonical name only without source text. Checked with the CLI against
`target/tsgo-pinned`: `duplicateIdentifierComputedName`,
`duplicateIdentifierDifferentSpelling`, `duplicateStringNamedProperty1`,
`numericClassMembers1` and four controls.

## §81 `module`, `process` and `Buffer` are resolved in JS files

check.rs's `check_value_identifier` declined every name with a special
cannot-find message (`require`, `module`, `process`, `Buffer`, …) in a JS
file. Its note said those were "globals upstream supplies through machinery
this port does not have". For `module` and `exports` that is no longer true:
the binder declares them as locals of a CommonJS file
(`declare_commonjs_variable`, `declareCommonJSVariable`, `binder.go:1608`)
exactly when native does. Native reports an unresolved `module` as TS2591,
for example `module.exports = ns` in a file that also has an ES `import`
(`jsDeclarationsExportFormsErr`, `modulePreserve4`). `process` and `Buffer`
come from `@types/node`, not from synthesis. The decline is narrowed to
`require`, whose call resolution is not this rule's. Checked with the CLI
against `target/tsgo-pinned` on the cases the original decline was measured
against (`modulePreserve4`, `maxNodeModuleJsDepthDefaultsToZero`,
`jsdocReferenceGlobalTypeInCommonJs`).

*Correction (§75).* The first gate of §75's span merged with main lost five
`duplicateIdentifierRelatedSpans*` cases. merge_conflicts.rs's
`duplicate_declaration_span` (TS2300 across files, and its TS6203 related
spans) took `error_span(declaration)`, which was a method signature's name
until §75. Native's `addDuplicateDeclarationError` reports at
`getAdjustedNodeForError`, i.e. `GetNameOfDeclaration`, never at the error
range. The default arm now takes the declaration's name.

## §82 TS2804, a private name shared by static and instance members

`checkObjectTypeForDuplicateDeclarations` with `checkPrivateNames`
(`checker.go:3196`, from `checkClassLikeDeclaration`) collects each private
name's static (2) and instance (1) uses. The first time a name reaches 3 it
reports `Duplicate identifier '{0}'. Static and instance elements cannot
share the same private name.` on every member of that name, through
`reportDuplicateMemberErrors(…, checkStatic=false)`. No emitter existed.
class_fields.rs's `check_private_names_static_and_instance` is the arm.
check.rs calls it beside `check_object_type_for_duplicate_declarations` for
class declarations and expressions. Checked with `--pretty true` against
`target/tsgo-pinned`: `privateNameDuplicateField`, `privateNamesUnique-3`
and three controls.

## §83 TS2675, extending a class whose constructor is private

`checkClassLikeDeclaration` calls `checkBaseTypeAccessibility`
(`checker.go:4481`) when `getBaseTypes` is not empty. The first construct
signature of the base constructor type is declared by the nearest class on
the base chain that has a constructor. When that declaration is `private`
and the `extends` entry is not within the base class
(`isNodeWithinClass`), it is TS2675, *Cannot extend a class '{0}'. Class
constructor is marked as private.*, at the entry, with the base fully
qualified. No emitter existed. heritage_conformance.rs's
`check_base_type_accessibility` runs ahead of that file's member-table
declines, as native's call precedes the relation. Checked with `--pretty
true` against `target/tsgo-pinned`: `extendPrivateConstructorClass`
(`abc.XYZ`), `classConstructorAccessibility2`, and two controls.

## §84 TS2371's span and a recovered body

`checkVariableLikeDeclaration` reports TS2371, *A parameter initializer is
only allowed in a function or constructor implementation*, with
`c.error(node, …)`. For a binding element the error range is its name
(`({ first = 0 }: …) => unknown` underlines `first`). check.rs's reporter
used the node's whole span; it now uses `error_span`. Its test
`NodeIsMissing(GetContainingFunction(node).Body())` also holds for a
recovered body with no width, as in `{ foo(x = 1), }` in an object literal,
where the parser expected a `{`. The reporter tested only `body.is_none()`
and now also counts a zero-width body as missing. Checked with `--pretty
true` against `target/tsgo-pinned`: `defaultValueInFunctionTypes`. The
object-literal arm is in a file with parse errors, which the CLI does not
show semantically, so the oracle measures `callSignaturesWithParameterInitializers2`.
