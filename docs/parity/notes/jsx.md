# Parity lane: JSX (tsr-2zk.8)

Lane notes for the `jsx` parity box. Judgment calls made while porting
typescript-go's JSX parsing and checking (pinned `vendor/typescript-go` @
`5b1047d`) are recorded here; the case list lives in
`docs/parity/lanes/jsx.txt`.

## 1. JSX parse recovery is a port of `parser.go:4736-5060`, not a reshaping

### Forcing constraint

The previous `crates/tsr-parser/src/jsx.rs` was an idiomatic JSX parser: it
parsed correct JSX correctly and bailed out of malformed JSX at the first
surprise (`parse_jsx_children` broke on any non-child token, attributes skipped
a stray token with "Unexpected token"). Upstream's recovery is not an
afterthought that can be bolted on; it is the *shape* of its functions:

- children are a list under `PCJsxChildren`, whose `isListElement` is always
  true — so while any JSX element is open, every list nested inside a `{…}`
  child aborts at the first token it cannot use (`isInSomeParsingContext`),
  rather than skipping it;
- attributes are `parseList(PCJsxAttributes, …)`, which reports `Identifier
  expected` and either aborts or skips by the same machinery;
- `parseJsxChildren` stops after a child whose closing tag names the *parent*
  (`<div><span></div>`), and the parent then **restructures** that child with a
  synthetic empty closing tag and steals its closing element, so TS17008 lands
  on the unclosed `span` and never twice;
- a missing closing tag at end of file reports TS17008/TS17014 at the opening
  tag, not at EOF.

Fourteen lane cases had parse-code diffs at 06f25e0 (EXTRA 1005/1003/1382,
MISSING 1109/17008/17002/17014/17015/17021). Patching each would have meant
re-deriving these decisions outside the algorithm that makes them, so the file
was rewritten function-for-function: every function in `jsx.rs` names its
`parser.go` counterpart.

### Representational differences kept, and why

- **`>` is split before every test.** Upstream's scanner never produces `>>`
  or `>=` (compounds come from `reScanGreaterToken`); this scanner joins them
  greedily. `at_jsx_greater_than` and `isListTerminator(PCJsxAttributes)` call
  `rescan_greater_than` first. That is upstream's token stream, not a
  heuristic: in every position these run, upstream holds a lone `>`.
- **Spans start at the token, not at leading trivia.** Upstream reports an
  unclosed fragment at `openingTag.Loc`, whose `pos` *includes* leading trivia
  (`tsxFragmentErrors` wants `(9,11)`, the end of the previous line's `>`). The
  fragment's full start is threaded through `JsxOpeningTag::Fragment` for that
  one report rather than changing what spans mean.
- **`lastChild.Children().End()`** has no node to read in a side-table AST; it
  is the end of the child's last child, or of its opening element
  (`jsx_children_end`), which is where the inner `parseJsxChildren` left the
  scanner.
- **An attribute initializer that is a sibling-recovery binary**
  (`<a b=<c/><d/> />`) cannot be stored: the generated `JsxAttributeValue`
  union has no binary arm. The diagnostics are reported; the initializer is
  dropped. Changing the union is an AST codegen change outside this lane.

### Entry points moved to upstream's

JSX now enters at `parseUpdateExpression` (a `<` followed by a name or `>`)
and `parseSimpleUnaryExpression` (any `<` as the operand of a prefix operator,
parsed with `mustBeUnary` so the sibling recovery cannot wrap it), both inside
`parse_unary_expression`. `parse_primary_expression` keeps `<` only for `.ts`
type assertions; in `.tsx` a `<` that is not JSX reaches the
missing-expression default exactly as upstream's `parsePrimaryExpression`
(no `<` arm) does — `var x = <:a …` now parses as upstream's types baseline
shows (`beginOfIdent1 : boolean`).

### Scanner changes this needed (JSX-only arms)

- `languageVariant == JSX` makes `</` (not `</*`) a single token in ordinary
  scanning (`scanner.go:778`). Without it `{ </div>` inside a child read `/`
  as a regular expression (EXTRA TS1161).
- `ScanJsxIdentifier` appends to the *decoded* value and the token keeps its
  escape flag, so `<a-b></a-b>` matches and `<a-c>` reports TS17021
  through `parseIdentifierNameErrorOnUnicodeEscapeSequence`.
- An unterminated JSX attribute string reports zero-width at the end of input
  (`scanString`'s `s.error`), not over the token.

### Known gaps left (not in this lane's files)

- **`finish()` deduplicates parser and scanner diagnostics by sorted
  position**, while upstream's guard is "same start as the *last reported*
  error". When a scanner error is reported, then a parser error elsewhere, then
  a parser error at the scanner error's position, upstream keeps both
  (`tsxAttributeInvalidNames` TS1351+TS1005 at `(10,10)`,
  `jsxAndTypeAssertion` two TS1005 at EOF). Needs the scanner's diagnostics
  routed into the parser's list in report order — parser box.
- **No JavaScript-file flag in the parser**: upstream skips JSX type arguments
  in JS (`contextFlags & NodeFlagsJavaScriptFile`), so `.jsx` parses
  `<Foo<number> />` as TypeScript here (`jsxCheckJsxNoTypeArgumentsAllowed`).
- **`parseTypeArguments` is `parseBracketedList(PCTypeArguments, …)`** upstream,
  which accepts `<>` and leaves TS1099 to the checker; `parse_type_arguments`
  always parses one type and reports TS1110 (`tsxTypeArgumentResolution`).

### How we would know this is wrong

A lane or corpus case whose only diff is a parse code inside a JSX element
after this commit is either one of the three gaps above or a divergence in a
function named here; compare against the named `parser.go` line, not against
the old behaviour.

### A types loss this exposed, not caused

`compiler/jsxNamespacePrefixInName{,React}` lose 16 `a : any` lines each: the
namespace part of `<a:element/>` now prints `undefined`. The parse of that
statement is unchanged; the later `<:a attr=… />` now parses as upstream does,
as `var beginOfIdent1 = <missing> < <missing>, a, attr = …` — upstream's own
`.types` baseline shows that declaration list. With `var a` in scope, the
conformance types producer's `check_expression` on the namespace identifier
resolves it. Upstream never asks: `getTypeOfNode` gates on
`IsExpressionNode`, which is false for an identifier under a
`JsxNamespacedName`, so it prints the error type (`any`). The fix is that gate
in `crates/tsr-conformance/src/types_producer.rs` (not this lane's file).

## 2. `checkJsxExpression`'s diagnostics live in the per-node walk

`checkJsxExpression` (`jsx.go:89`) is a type function that also reports
TS18007 (`checkGrammarJsxExpression`) and TS2609. This port computes types in
`check_expression`, which inference and contextual typing call repeatedly, and
reports from the per-node walk in `check.rs`, which visits each node once. So
the two reports are `check_jsx_expression`, a `JsxExpression` arm of that walk;
the spread child's type is the memoised `check_expression`, and an `errorType`
(a gap) reports nothing. `grammarErrorOnNode`'s parse-error gate is
`file_has_parse_errors`.


## 3. The JSX factory is a name per file, not an entity

### Forcing constraint

`markJsxAliasReferenced` (`checker.go:28502`) was ported only as its TS2874
report, under `jsx: react`, for elements. Its other effect — `isUse` marking
of the factory's root symbol — is what keeps `import React = require("react")`
from being TS6133 under `noUnusedLocals` in **every** emit mode, and
fragments resolve a second name (the `@jsxFrag` / `jsxFragmentFactory` root,
then the element factory's root). At round 2's baseline that was 13 lane
cases (`unusedImports13`–`16` EXTRA 6133, `inlineJsxAndJsxFragPragma`,
`jsxFragmentAndFactoryUsedOnFragmentUse` EXTRA 6192, three `jsxFactory*`
cases with a wrong TS2874 or a missing TS2552).

### What was ported, and the representational choice

`crates/tsr-checker/src/jsx_factory.rs` holds `getJsxNamespace`,
`getJsxFactoryEntity` (its root) and `markJsxAliasReferenced` whole. Upstream
parses each factory into a synthetic entity with `parseIsolatedEntityName`
and caches it in `sourceFileLinks`; every reader here takes only
`GetFirstIdentifier` of it, so this port keeps the root *string*: per file
from the parser's `FileReferences` (`@jsx`, `@jsxFrag`, `@jsxImportSource`,
`@jsxRuntime`, last pragma wins as in `GetPragmaFromSourceFile`), per checker
from the options. Rejected: storing the entity — nothing reads its tail.

`parseIsolatedEntityName` matters for its *failures*: `jsxFactory:
Element.createElement=` does not parse, so upstream falls back to `React`
(`jsxFactoryNotIdentifierOrQualifiedName{,2}`). The old `split('.')` took
`Element`. The parse is identifier names joined by `.` (keywords allowed, so
`@jsxfrag null` names `null`, which the fragment arm exempts). It exists twice
— `tsr_parser::pragma::isolated_entity_name_root` and
`jsx_factory::isolated_entity_name_root` — because the checker does not depend
on the parser crate; adding that dependency for eight lines was rejected.

A miss under `jsx: react` goes through `onFailedToResolveSymbol`'s lib and
spelling arms before the TS2874 fallback, so `jsxFactory: createElement` with
`frameElement` in the DOM lib is TS2552 (`jsxFactoryIdentifierWithAbsentParameter`).
The `checkAndReportErrorFor*` arms before those are not ported; none fires on
a tag name in the corpus.

The old `file_has_parse_errors` gate on TS2874 was removed: TS2874 is a
`resolveName` error, not a grammar error, and upstream reports it in files
with parse errors (`tsxErrorRecovery3`).

### The decline kept

`getJsxNamespaceContainerForImplicitImport` returns early only when the
automatic runtime's module *resolves*; this port returns whenever
`GetJSXImplicitImportBase` selects the automatic runtime (host method
`jsx_implicit_import_base`). The difference is the TS2875 population, where
upstream still marks and reports `React`; there this port does neither, which
was its behaviour before (nothing marked the factory at all). Lifting it needs
module resolution by name from the checker, filed with the round-2 report.

### `checkJsxFragment`'s TS17016/TS17017

Reported from the per-node walk on the `JsxFragment` (§2's split): under a
JSX transform, a `jsxFactory` option or `@jsx` pragma without a fragment
factory. The pragma test is *presence* (`has_jsx_pragma`), parseable or not.
`getJSXFragmentType`'s TS2879 is not ported: it lives inside fragment
signature resolution, which this port does not have.

### How we would know this is wrong

An EXTRA TS6133 on a JSX factory import under classic runtime, or a TS2874
naming a different root than the baseline, is a divergence in
`jsx_namespace_at`; an EXTRA TS6133 under `jsx: react-jsx` with an unresolved
runtime module is the decline above.

## 4. JSX `checkSpreadPropOverrides` reports from the attributes walk

`createJsxAttributesTypeFromAttributesProperty` (`jsx.go:709`) keeps, under
`strictNullChecks`, an `allAttributesTable` of every attribute so far (a later
attribute of the same name replaces the earlier) and runs
`checkSpreadPropOverrides` (`checker.go:13371`) for each valid spread: TS2783
on the overwritten attribute. It is a type function upstream; here it is
`check_jsx_spread_property_overrides`, the per-node walk's `JsxAttributes` arm
(§2's split). Differences from the object-literal arm in
`crate::spread_overrides`, all upstream's: no
`tryMergeUnionOfObjectTypeAndEmptyObject`, no assignment-target exemption, and
the spread's properties are read from its **apparent** type, because
`getPropertiesOfType` reduces to it — `{...props}` with `props: T extends { x:
number }` overwrites `x` (`tsxGenericAttributesType1`). `getReducedType` is not
ported; it only turns a disjoint-discriminant intersection into `never`, which
`isValidSpreadType` rejects in either form. Two spreads overwriting one
attribute are one line, as the baseline folds upstream's two diagnostics that
differ only in related information.

## 5. `getJsxNamespaceAt`'s first and last roads

Road 1, `getJsxNamespaceContainerForImplicitImport` (`jsx.go:1451`), is now
`jsx_implicit_import_container`: `GetJSXRuntimeImport` of the host's
`GetJSXImplicitImportBase`, resolved as an ambient module or through the
host's record of the loader's synthetic runtime import, then past its
`export =`. When it resolves, road 2's name is not consulted, as upstream.
The loader records only the option-level runtime, so a runtime named only by
an `@jsxImportSource` pragma does not resolve here and falls through to road
2 — the one-sided failure the TS7026 rule already documents.

The `JSX` export found on either road is resolved through aliases
(`resolveSymbol`, `jsx.go:1321`), so `export import JSX = …` works when
`resolve_alias` follows it. Preact's runtime (`export import JSX =
JSXInternal`, where `JSXInternal` is itself a named import from `'..'`) is
still declined by `resolve_alias`, which is why
`jsxNamespaceImplicitImportJSXNamespace` keeps its EXTRA TS7026 — reported
to the integrator, not worked around.

Road 3, the global fallback, is `getGlobalSymbol(JSX, Namespace)`
(`jsx.go:1334`): the globals table, not a scoped lookup from the tag. A
module declaring its own `namespace JSX` is invisible to it, which is why
upstream reports TS7026 in `jsxPropsAsIdentifierNames`. The old scoped
lookup was not upstream's and found it.

`markJsxAliasReferenced`'s early return stays keyed on the implicit-import
*base* (§3's decline), not on road 1 resolving: with a pragma-only runtime
the container cannot resolve here, and falling through would start marking
and reporting `React` where upstream resolved the runtime and returned.

## 6. TS2786 on the one signature this port resolves

`checkJsxOpeningLikeElementOrOpeningFragment` (`jsx.go:131`) relates the
resolved signature's return type to the bound its `getJsxReferenceKind`
selects (`checkJsxReturnAssignableToAppropriateBound`, `jsx.go:168`):
`JSX.Element | null` for a function, `JSX.ElementClass` for a class, their
union for a mixed tag. Ported as `check_jsx_component_bound`, reporting
TS2786 on the tag name (the chained detail lines are not modelled; the code
and span are upstream's).

The signature is the one `jsx_attributes_context` publishes in
`resolved_call_signatures`: a single candidate, instantiated when generic —
what `resolveCall` returns for a one-candidate list whether or not the
arguments fit. Declined, each because the faithful answer lives elsewhere:

- **overloads and union tags** — the candidate is `resolveCall`'s choice
  (calls lane; `tsxElementResolution9` keeps two missing TS2786);
- **`JSX.ElementType` in scope** — upstream takes the `elementTypeConstraint`
  branch instead (tag type against `ElementType`, instantiated with
  defaults); not ported (`jsxElementType*`);
- **intrinsic tags** — the fake signature returns `JSX.Element`, which the
  mixed bound always accepts, so nothing is lost by skipping them;
- **an `errorType` bound or return** — relates to everything upstream.

A false TS2786 here would mean the published signature is not upstream's
resolved one; compare against `resolveJsxOpeningLikeElement`, not this rule.

## 7. TS2710 from the attributes walk

`createJsxAttributesTypeFromAttributesProperty` (`jsx.go:819-826`) reports
`'children' are specified twice` on the attributes node when an explicit
attribute names the children property and the element's body has semantic
children (`GetSemanticJsxChildren`), unless a spread typed `any` (or the
error type, which `IsTypeAny` also accepts) made the attributes type `any`.
Same §2 split as §4: `check_jsx_children_specified_twice`, on the
`JsxAttributes` arm of the walk. A `children` that arrives through a spread
is not explicit and does not report, as upstream.

## 8. TS2339 from `getIntrinsicTagSymbol`

The arm after TS7026's in `getIntrinsicTagSymbol` (`jsx.go:1228-1250`): with
`JSX.IntrinsicElements` resolved, an intrinsic tag (opening, self-closing or
closing — §255's node set) that is neither a property nor covered by an
applicable index signature reports `Property 'x' does not exist on type
'JSX.IntrinsicElements'` on the element. It is not inside `noImplicitAny`.
`check_jsx_intrinsic_tag_exists` requires the table to be completely
enumerable (`get_property_names_of_type` answering `Some`); a name missing from
an unresolved table is not evidence and declines.

### Round 1's TS2604 stays unlanded, and why

Round 1's `resolveJsxOpeningLikeElement` no-signature arm (1da6e97, reverted
as 6eb0a11) still reports a false TS2604 on
`compiler/reactSFCAndFunctionResolvable` after the round-2 calls merge. The
cause is not `getUnionSignatures`: the conditional `cond ? Radio : Checkbox`
reduces to `React.SFC` (subtype reduction), and `signatures_of_type_kind`
answers a *complete, empty* call list for that type — `React.SFC` is
`type SFC<P = {}> = StatelessComponent<P>`, an interface with a call
signature, reached through `import * as React` from `react16.d.ts`. A small
local repro of the alias-with-defaults shape resolves its signature, so the
gap is in how that declaration file's reference is resolved. Until the
producer answers the signature (or `None`), the arm cannot trust an empty
list.
