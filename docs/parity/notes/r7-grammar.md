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
