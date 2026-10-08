# r5-jsdoc2 — JSDoc type hosting (round 5)

Lane `tsr-2zk.1029`, epic `tsr-2zk`. Issues `.16.98`, `.16.105`, `.16.106`,
`.16.107`, `.16.120`, `.16.147`, `.16.163`, `.16.38`. Native source is
`vendor/typescript-go` @ `5b1047d`. Continues [r4-jsdoc](r4-jsdoc.md).

Frozen baseline (`e7b08f4`): types 543,906 RIGHT of 552,533 aligned lines
(plain and configured keys together), diagnostics 5,327 RIGHT / 5,577
EMPTY_RIGHT of 12,238 rows.

## Ownership as worked

The lane owns the `jsdoc_*.rs` files. Every read site this round's issues
name is outside them: the parser (`tsr-parser`, held for `tsr-2zk.17.1`),
`signatures.rs`, `optionality.rs`, `symbols.rs`, `check.rs`'s shared
helpers. So the commit holds the reparse queries in `jsdoc_params.rs` — the
one place ADR-0046 says the reparse is answered — and each consumer is a
one- or two-line diff under `docs/parity/notes/r5-jsdoc2-*.diff`, measured
together (§1) and separately where they can land separately.

## 1. Reparse queries and their consumers (`.16.120`, `.16.38`)

**Landing order.** Apply on this lane's commit, in order:
`r5-jsdoc2-function-expression-jsdoc.diff` (parser, independent),
`r5-jsdoc2-reparsed-parameter-consumers.diff`,
`r5-jsdoc2-accessor-annotation.diff`. The committed queries carry
`#[expect(dead_code)]` until their consumer lands; each diff removes the
expectation it fulfils, so a diff applied without its predecessor fails the
lint instead of silently leaving a query unused. The three applied in that
order reproduce byte-for-byte the tree measured in §1.4.

### 1.1 A function or arrow expression's own comment (`.16.120`)

**Forcing fact.** `foo(/** @param {number} x */ (x) => x)`: native types `x`
as `number`; TSR leaves it `any`. Native attaches the comment to the arrow
itself — `parseParenthesizedArrowFunctionExpression` (`parser.go:4434`),
`parseSimpleArrowFunctionExpression` (`:4548`) and `parseFunctionExpression`
(`:5711`) all end in `withJSDoc` — and `reparseHosted` then writes the tags
onto the arrow's own parameters (`getFunctionLikeHost(fun) == fun`). TSR's
parser attached JSDoc only to declarations, statements, members, parameters
and parenthesized expressions, so a comment before an expression-position
function reached nothing. The checker side needed no change:
`jsdoc_reparsed_function` already treats the function as its own first host,
and `jsdoc_this_parameter_type` / `jsdoc_return_annotation` walk from it.

**Diff** (`r5-jsdoc2-function-expression-jsdoc.diff`, parser). Native's
`jsdocScannerInfo` is taken before the `async` and the comment is parsed only
in `withJSDoc`, after the node is finished. The diff mirrors that split:
`leading_jsdoc_marker` records where the comment is (no parse);
`parse_jsdoc_at` parses it once the arrow or function expression exists.
A speculative arrow parse that rewinds therefore never parses (or reports)
a comment it does not keep. A comment that also precedes the enclosing
statement is parsed twice, as natively; `JSDocTable`'s diagnostics are
already de-duplicated by span and code (`parser.rs:384`).

**Alternative rejected.** Reading the comment from the checker by source
position (no parser change). It would be a second, position-based attachment
rule beside the parser's, and the parser is where native decides it.

### 1.2 The reparsed `?` and `param.Type` (`.16.120`, `.16.147`)

Native `makeQuestionIfOptional` gives `@param [x]` / `@param {T=} x` a
reparsed `QuestionToken`, and `reparseJSDocTypeLiteral` sets `param.Type`.
Every native consumer reads those through the parameter; TSR's consumers read
only the written `question_token` / `r#type`. New owned queries
(`jsdoc_params.rs`):

- `jsdoc_reparsed_parameter_question(parameter)` — the replay's `question`;
- `jsdoc_reparsed_parameter_type(parameter)` — the parameter's own `@type`,
  else the matched `@param`'s type (a new `r#type` field on the replay slot);
- the replay's `return_type` — the first typed `@returns` of an unannotated
  function, or a getter's own `@type` (`reparseHosted`'s `KindGetAccessor`
  case, which previously only consumed the tag).

Consumers (`r5-jsdoc2-reparsed-parameter-consumers.diff`):

| Site | Native | Change |
|---|---|---|
| `optionality.rs` `is_optional_declaration` | `isOptionalDeclaration` reads `QuestionToken` | parameter arm asks the reparsed `?` |
| `signatures.rs` signature loop | `isOptionalParameter` | reparsed `?` makes the slot optional |
| `signatures.rs` `@param` arm | `getTypeOfParameter` → `addOptionality` | bracketed `@param` adds `undefined` under strict, as `{T=}` already did; the slot reuses the tag's type node for printing (`serializeTypeForDeclaration`), except JSDoc-only node kinds (`nodecopy.go` rewrites, `.16.147`, not ported) |
| `signatures.rs` `has_context_sensitive_parameters` | `HasContextSensitiveParameters` reads `param.Type()` | a parameter typed by its reparsed `@param` is not context-sensitive |

### 1.3 The setter's value parameter (`.16.38`)

`r5-jsdoc2-accessor-annotation.diff` (`symbols.rs` `accessor_annotation`):
when no annotation is written, fall back to the owned
`jsdoc_accessor_annotation` — `getAnnotatedAccessorTypeNode`
(`checker.go:20106`): the getter's reparsed `Type`, or the type of
`GetSetAccessorValueParameter` (the first non-`this` parameter), which in JS
is its reparsed `@param`.

### 1.4 Measured

All three §1 diffs applied together on this branch's commit, against the
frozen baseline, unfiltered:

- **types** 543,906 → **543,978 RIGHT (+72)**; gap 1,015 → 993, wrong
  7,612 → 7,562. Zero RIGHT → non-RIGHT lines.
- **diagnostics** RIGHT 5,327 → **5,328** (thisInFunctionCallJs); zero RIGHT
  or EMPTY_RIGHT rows changed verdict.
- Lines converted, by case: thisInFunctionCallJs 12,
  contextuallyTypedParametersOptionalInJSDoc 12, paramTagTypeResolution 9,
  contravariantOnlyInferenceWithAnnotatedOptionalParameterJs 9,
  contravariantOnlyInferenceFromAnnotatedFunctionJs 6,
  jsDeclarationsClasses 5,
  jsDeclarationEmitExportAssignedFunctionWithExtraTypedefsMembers 5,
  declarationEmitObjectLiteralAccessorsJs1 3, arrowFunctionJSDocAnnotation 3,
  contextualTypeFromJSDoc 2, checkJsdocParamOnVariableDeclaredFunctionExpression
  2, jsDeclarationsGetterSetter 1, declarationEmitClassSetAccessorParamNameInJs
  1/2/3 (1 each). The accessor diff (§1.3) accounts for the last six cases'
  14 lines; the parser diff alone (with the consumers absent) converted 27 of
  the `.16.120` lines.
- Cases now fully RIGHT: arrowFunctionJSDocAnnotation,
  thisInFunctionCallJs, contextuallyTypedParametersOptionalInJSDoc,
  contravariantOnlyInferenceWithAnnotatedOptionalParameterJs.
- **Perf** (median child CPU, new/old, 21 samples, `diagnostics_match`
  true): domain-model 0.989, generic-imports 1.008.
- `cargo test --workspace --release` passes with the set applied.

Still WRONG in `.16.120`: contravariantOnlyInferenceFromAnnotatedFunctionJs
(4 lines: two GAP rows its TypeScript twin shares, so not a JSDoc question,
and two where the `@template {Record<string, unknown>} B` constraint is not
printed — the JSDoc template-constraint read, `declared.rs`), and
amdLikeInputDeclarationEmit (12: the AMD `define` callback's parameter
typing, not a hosting question).

## 2. Held: checking reparsed JSDoc type nodes (TS2344 from r5-constraints2)

**Native.** The reparsed type nodes are ordinary children of their hosts, so
`checkSourceElement` visits them (`checkVariableLikeDeclaration`,
`checkSignatureDeclaration`, `checkTypeAliasDeclaration`), and every
type-node rule — `checkTypeArgumentConstraints`' TS2344 among them — fires
inside JSDoc.

**Diff** (`r5-jsdoc2-jsdoc-type-walk.diff`): a new
`jsdoc_reparsed_type_nodes(node)` (the hosted `@type` of a variable, a
parameter's reparsed type, a function's reparsed `@returns`, every typedef's
type expression), visited by a one-line hook in `check_node`.

**Finding.** The walk alone reports nothing: `source_file_of_for_diagnostics`
(`check.rs`) climbs parents to the `SourceFile` and dead-ends at the JSDoc
root, which has no parent edge (`attach_jsdoc`), so **every diagnostic
anchored inside a comment is silently dropped** — today as well, for any
rule that ever reaches a JSDoc node. Crossing to the host there (as
`symbols.rs::source_file_of` already does) is the faithful change, and it
makes checkJsdocTypeTag4's first TS2344 appear.

**Why held.** Measured with the walk and that change on top of §1:
diagnostics −36 rows (34 EMPTY_RIGHT → EMPTY_WRONG, 2 RIGHT → WRONG), +1
(thisInFunctionCallJs, from §1). The losses are rules that now see JSDoc
nodes and are wrong there, each an unported piece:

| Diagnostic | Cases (sample) | Missing piece |
|---|---|---|
| TS2304 / TS2503 on names inside JSDoc | inferThis, importTag2/3/9/16/18/19/20/25, jsdocAccessEnumType, varRequireFrom* | the JSDoc host hop in name resolution (`r4-jsdoc-scope-hop.diff`, `.16.107`) |
| TS1228 | returnTagTypeGuard | not root-caused; hypothesis: the type-predicate position rule does not see a `@returns {x is T}` node as a return-type position |
| TS17020 | checkJsdocTypeTag3 | not root-caused; hypothesis: the grammar check's position test treats the JSDoc node differently from native's reparsed clone |

**Reopening condition.** Land the scope hop (§2.1 of r4-jsdoc) first; then
re-measure this diff; the remaining TS1228/TS17020 rows are position tests to
teach about reparsed positions. checkJsdocTypeTag4's second row also needs
the typedef's `@template {string}` constraint read
(`constraint_check_type_parameters`, `declared.rs`), and
unmetTypeConstraintInJSDocImportCall / extendsTag5 need
`check_type_argument_constraints` to cover `ImportType` and heritage
`ExpressionWithTypeArguments` (constraints lane).

## 3. Remaining clusters

| Issue | Cases | Hypothesis | Site (owner) |
|---|---|---|---|
| `.16.107` host scope | 7 | `r4-jsdoc-scope-hop.diff` still applies cleanly at `e7b08f4`; held for r4-jsdoc §2.4's four losses (contextual and node-reuse pieces) | binder (main) |
| `.16.98` hosting | 8 | `r4-jsdoc-property-type.diff` still applies; needs §2.1 first. Catch-clause `@type`: `symbols.rs` early return (r4-jsdoc §5) | `symbols.rs` (main) |
| `.16.105` default export | 7 | `symbol_chain`'s `import("./a").` qualifier rule (r4-jsdoc §3) | `checker.rs` printer |
| `.16.106` class `@template` | 7 | subsumed by the merged type-parameter stack; remaining rows are overload/extends rows (`.16.163`) | `declared.rs` |
| `.16.147` reuse of JSDoc nodes | 5 | `nodecopy.go:480-522` rewrites (`T=` → `T \| undefined`, `?T` → `T \| null`, `*` → `any`) on reuse; §1.2 reuses plain nodes only | `node_reuse.rs` |
| `.16.163` `@overload` | 4 | `reparseUnhosted`'s overload arm: one signature per tag for function, method and constructor declarations outside object literals | `signatures.rs` |
