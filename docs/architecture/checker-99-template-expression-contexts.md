# Template expression contexts and template union reduction

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 7332f284 (code identical to 2671c87c): 456,480/478,855 matching
assertions. Unit: tsr-6.11 (semantic template literal types).

## checkTemplateExpression's template arm

checkTemplateExpression (checker.go:7976) answers, in order: the folded constant
string, a template literal type, or `string`. The template arm fires for
`isConstContext || isTemplateLiteralContext || someType(contextualType,
isTemplateLiteralContextualType)` (checker.go:7997–8010). The port had only the
const-context leg, and returned `errorType` (a gap) for any template under an
`as`/`<T>` assertion or an element-access argument — checker-notes-narrow.md §24's
decline, written before template literal types existed in this port. Both
declines are now replaced by the native predicates:

- isTemplateLiteralContext: an element-access argument, through parentheses.
- isTemplateLiteralContextualType: a string-literal or template-literal
  constituent, or an instantiable non-primitive whose base constraint (unknown
  when absent) may be string-like.

**One deviation, measured.** Upstream's inference pass checks a call argument
against the uninstantiated parameter type, so `g2<T extends string>(x: T)` sees
`T` and its string constraint. This port checks arguments in one pass, by which
time the contextual type may be the fixing instantiation. A negative answer
therefore re-asks with `contextual_prefers_uninstantiated`, the same retry the
literal-freshness question uses (objects.rs, §946). Without the retry the unit
measured +22 against +41 with it, both with zero RIGHT losses; the 19 rows the
retry adds are `g2(\`xyz-${s}\`)`/`takesLiteral`-shaped generic calls. It would
be wrong if an instantiated contextual type were non-template while the written
parameter's constraint were string-like *and* native answered `string`; no such
row appeared in the full run.

## Template sources against decidable targets

structuredTypeRelatedToWorker's template-source arm (relater.go:3772) relates a
template to a non-object, non-template target only through a distinct base
constraint. No target-side arm applies to a flag-decidable target (literals,
primitives), so a failed constraint is `False`. The port returned `Unknown`
there, which made `cond ? 'a' : \`foo${s}\``'s subtype reduction undecidable and
the conditional a gap. The arm now answers it. Targets that are not flag-decidable
(type parameters, conditionals, objects without members tables) keep the existing
undecidable fallthrough.

## Literal reduction with templates

removeRedundantLiteralTypes (checker.go:25838) removes template-literal and
string-mapping constituents beside `string`, and getUnionType calls it whenever
those flags are present (checker.go:25674). The port's copy recorded that template
types "do not exist in this port" — no longer true since the semantic template
factory — so `string | \`foo${string}\`` survived literal reduction. Both clauses
are now ported, as is removeStringLiteralsMatchedByTemplateLiterals
(checker.go:25857): a string literal matched by a pattern template (relater
template-target arm under assignability) or a member of a pattern string mapping
is dropped beside it.

## Verification

Full scorepair of this unit against cec7cef5: 457,691 right (+50):
49 WRONG→RIGHT, 1 GAP→RIGHT, zero RIGHT losses, no GAP→WRONG. Moved:
templateLiteralTypes2 21,
templateLiteralTypes3 13, indexSignatures1 7, plus mappedTypeConstraints2,
templateLiteralIntersection{2,3} and singletons. Regression tests:
crates/tsr-checker/tests/template_expression_contexts.rs.

## Not done

`nonWidening(cond ? 'a' : 'b')` still answers `string` where native keeps
`"a" | "b"`: literal freshness of a conditional in a generic argument position
is an inference question outside this unit, and it is what keeps
templateLiteralTypes2's `y2` rows wrong. Remaining template rows in
templateLiteralTypes3/Patterns are alias display (`Pat<T>` vs `` `${T}` ``),
intersections inside holes (`${string & {}}`), conditional evaluation over
template checks and union-of-template inference; tsr-6.11 stays open.
