# Lane `js` notes (`tsr-2zk.5`)

Judgment calls made while porting the JavaScript / JSDoc lane, in commit order.
Pinned upstream: `vendor/typescript-go` @ `5b1047d`. Baseline: lane list at
`06f25e0` (187 cases; 126 failing diagnostics, 118 failing `checker_types`).

## 1. JSDoc parse errors reach checked JavaScript files

**Forcing constraint.** Upstream moves every diagnostic raised inside a JSDoc
comment into `Parser.jsdocDiagnostics` (`parser/jsdoc.go:171`), and
`getBindAndCheckDiagnosticsWithChecker` appends `SourceFile.JSDocDiagnostics()`
for a checkJs file (`compiler/program.go:1366`). This port truncated them
(`docs/architecture/jsdoc.md` "Diagnostics are discarded"), so every JSDoc
syntax error in the lane (`TS1003`/`1005`/`1109`/`1110` in comments) was missing.

**What was built.**

- `parse_jsdoc_comment` drains the comment's diagnostics into
  `Parser::jsdoc_diagnostics`; `finish` hands them out on
  `JSDocTable::diagnostics`. Upstream keeps them only when the parser's context
  is a JavaScript file; this parser's `ScriptKind` has no JavaScript arm
  (`.js` parses as TypeScript, `.jsx` as TSX), so they are collected for every
  file and the *consumer* applies the gate — `bind_and_check_diagnostics`
  (`tsr-compiler/src/program_diagnostics.rs`) adds them for a JS file with
  checkJs on, after the plain-JS early return, before directives, exactly where
  upstream appends them. Cost: a `Vec` that stays empty unless a comment has an
  error.
- Like upstream's, the list is **not** rewound by speculative parsing; a comment
  re-read after a rewind reports twice, and `finish` drops repeats (same span and
  code), which is what upstream's `SortAndDeduplicateDiagnostics` does to them.
- Scanner diagnostics raised inside a comment are still dropped: the scanner
  rewinds its own list on `restore`, and moving them needs a scanner accessor
  (scanner crate, not this lane). No lane case needs them.

**Making the reported errors upstream's.** Surfacing the list exposed every
place the JSDoc parser objected where upstream does not. Each was ported, not
filtered:

| construct | upstream | fix |
|---|---|---|
| `@import` with no clause | `tryParseImportClause` (`parser.go:2332`): clause and `from` only after a name, `*` or `{` | `parse_import_tag` |
| `@implements` with no name | `parsePropertyAccessEntityNameExpression` requires the head name (`TS1003`) | reports, still returns no expression (no consumer distinguishes) |
| `` @param `x` `` | `parseBracketNameInPropertyAndParamTag` accepts a backquoted name; a missing *param* name is not reported (nil message), a missing *property* name is | `parse_bracket_name_in_property_and_param_tag(target)` |
| `expect_jsdoc` | `parseExpectedJSDoc` reports `'{0}' expected.` (`TS1005`) | was `Identifier expected` |
| `@template [T=D]` | `parseTemplateTagTypeParameter`: default only inside brackets, typed by `parseJSDocType`; `[T]` → `'=' expected`, `[T=]` → `Type expected`; a nameless parameter is dropped; name message `TS1069` | `parse_template_tag_type_parameter`; the old unbracketed `T=D` default (not upstream grammar) is gone |
| `@extends { A }` | `parseOptional(OpenBrace)` scans on under ordinary rules, skipping the space | `skip_whitespace` inside the braces |
| `@see {@link A}` / `@see Name` | `parseSeeTag` takes a name reference only for a name or `{` + name; a `{@link` is comment text | `parse_see_tag_name_reference`; the parsed name is not stored (the AST slot is a `TypeNode`, `JSDocNameReference` is not one, nothing reads it) |
| `{@link this.#c}` | `parseJSDocLinkName`: `.` before a private name leaves the right side missing; `#name` qualifies | `parse_jsdoc_link_name` |
| `@return {x is T}` | `parseJSDocType` calls `parseTypeOrTypePredicate` | `parse_jsdoc_type_at` |
| `*`, `*=`, `?T`, `??T`, `!T`, `T!`, `T?` | `parseNonArrayType` / `parsePostfixTypeOrHigher` JSDoc arms, in every file | `parse_jsdoc_prefix_type` / `parse_jsdoc_postfix_type`, hooked from `types.rs` |
| `T?` in a tuple | `parseTupleElementType` turns a postfix nullable into an optional element | `parse_tuple_element` |
| `Object.<K, V>` | `parseEntityName` stops at `.` followed by `<` | `parse_entity_name` |
| `{function}` | `parseNonArrayType` default → `parseTypeReference` with reserved words | `FunctionKeyword` arm only — see below |

**Rejected / deferred, with the number that refused it.**

- *Every reserved word as a type reference* (upstream's full behaviour): turned
  `conformance/varianceAnnotationsWithCircularlyReferencesError:0:0` RIGHT →
  WRONG. Cause is not the arm: `parse_type_parameters` has no list recovery, so
  `type T<in in>` falls out of the list at the second `in` and the alias body
  parse meets it, where upstream's `parseBracketedList(PCTypeParameters)` parses a
  second (nameless) parameter and never asks for a type. Only `function` — the
  reserved word `isStartOfType` itself names — is taken. Revisit when the
  parser lane ports type parameters onto `parse_delimited_list`.
- *Attaching end-of-file JSDoc* (`withJSDoc(eof, endJSDoc)`, `parser.go:438`): the
  comment is now parsed (its diagnostics convert `jsdocPrivateName2`), but not
  attached. Attaching binds a trailing `@typedef`, and
  `compiler/jsdocResolveNameFailureInTypedef:0:0` went RIGHT → WRONG
  (`(x: Ty) => void` printed `(x: any) => void`): `type_alias_body`
  (`declared.rs`) declines non-literal typedef bodies, so the alias with an
  unresolvable body answers `any` where upstream keeps the unresolved type under
  the alias name. Attach once typedef alias bodies are complete (names/decls
  lanes own `declared.rs`).
- *`parse_primary_expression`'s `Expression expected` at end of file*: upstream
  reports it zero-width at the EOF token's full start
  (`createIdentifierWithDiagnostic`, `parser.go:5862`); this port's fallback in
  `expression.rs` reports at the token. `report_missing_identifier` already has
  the rule. This is what still keeps `importTag10`/`11`/`12` wrong (`(3,2)` for
  `(2,11)`/`(2,15)`/`(2,20)`); `expression.rs` is the parser lane's.

**How we would know we were wrong.** A checkJs case whose baseline is clean
going EMPTY_WRONG with a `TS1xxx` inside a comment: that is a JSDoc grammar
divergence, and the fix is the grammar, never a filter on the list.

## 2. `@template` modifiers are any modifier keyword

`parseTemplateTagTypeParameter` calls `parseModifiersEx(false, true, false)`
(`jsdoc.go:1260`), so `@template private T` is modifier `private` on parameter
`T` — the checker's TS1273 rejects it — not a parameter named `private`. This
port took only `const`/`in`/`out`. Now every modifier keyword `isModifierKind`
lists except `default` (whose `nextTokenCanFollowDefaultKeyword` wants a
declaration keyword) is taken when a name follows on the line.

No score movement: `jsdocTemplateTag7:0:1` moves from `<private>(x: T) => T` to
`any` (both WRONG) because the checker's JSDoc-template signature road declines
a modifier-bearing parameter, as it already did for `<in T>`
(`jsdocTemplateTag8:0:47`); that road is `signatures.rs` (calls lane). The
grammar checks TS1273/TS1274/TS1277 on type-parameter modifiers are not
implemented for any file kind.
