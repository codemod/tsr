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
`check_enum_member_name`. Upstream's `checkEnumMember` never checks the name,
so its expression is never resolved. This port's check walk resolves every
value identifier, so `[e]` still draws TS2552/TS2304 and the five cases stay
WRONG on that extra line. The faithful hook is a third arm in
`names_in_unchecked_region` (`name_slots.rs`, not this lane's): a
`ComputedPropertyName` whose parent is an `EnumMember` is a region native never
checks. Routed in the report.

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
