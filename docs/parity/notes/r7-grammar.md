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
