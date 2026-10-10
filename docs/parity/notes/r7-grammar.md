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

**Held: `binder.go:1311`'s TS1359 arm** (`await` as an identifier in an
`AwaitContext`, `strict_mode.rs`'s `check_contextual_identifier`). Ported and
measured on this base at 15 cases gained and 8 lost: `var v: await;` inside an
async function (`asyncFunctionDeclaration13`, `asyncArrowFunction10`,
`parser.asyncGenerators.*`, `awaitAsTypeIsOk`) drew TS1359 because this
parser's `parse_type` does not clear the await and yield contexts
(`parseType`'s `setContextFlags(NodeFlagsTypeExcludesFlags, false)`,
`parser.go:2608`), so a type's identifiers carry `AWAIT_CONTEXT`. Routed to
r7-parser; the arm lands once types are parsed outside the contexts. The
`yield` arm and `checkGrammarYieldExpression`'s TS1163 read
`NodeFlagsYieldContext`, which the parser now stamps, and wait on the same fix.

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
