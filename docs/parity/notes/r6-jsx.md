# r6-jsx — JSX diagnostics root causes (`tsr-2zk.1141`)

Round-6 lane notes, continuing `r5-jsx3.md`. Native source is
`vendor/typescript-go` @ `5b1047d`, `internal/checker/jsx.go` unless noted.
The lane owns `jsx_attributes.rs`, `jsx_component.rs` and `jsx_factory.rs`;
everything else is shipped as a measured diff (§9).

## 0. The board at the frozen base

Frozen base `c7736c8` (tip of `claude/beautiful-shannon-ar5gh0`, batch BF).
61 diagnostics rows whose case name contains `jsx` or `tsx` are WRONG or
EMPTY_WRONG (the brief's 55 counted plain keys; configured keys add six).
Each was compared row by row (expected against actual, unfiltered) and
classified by the native operation that owns the missing or extra row:

| Cluster | Native operation | Cases (rows at the base) |
|---|---|---|
| TS2769 over JSX candidates | `resolveCall`/`chooseOverload` for a JSX tag | `jsxChildrenArrayWrongType`, `jsxChildrenWrongType`, `jsxElementType` (70:2, 72:20), `reactDefaultPropsInferenceSuccess`, `spellingSuggestionJSXAttribute`, `tsxNotUsingApparentTypeOfSFC`, `checkJsxChildrenCanBeTupleType`, `contextuallyTypedStringLiteralsInJsxAttributes02`, `tsxElementResolution9`, `tsxStatelessFunctionComponentOverload4`, `…Overload5`, `…WithTypeArguments4` |
| Type arguments on a tag (TS2558/1099/2344) | `resolveJsxOpeningLikeElement`'s intrinsic arm, `reportCallResolutionErrors` | `jsxIntrinsicElementsTypeArgumentErrors`, `jsxUnclosedParserRecovery`, `tsxTypeArgumentResolution` |
| Parser recovery and JSX grammar (TS1003/1005/1109/17004/17008…) | parser (main) | `conflictMarkerTrivia3`, `jsFileCompilationTypeArgumentSyntaxOfCall`, `parseJsxElementInUnaryExpressionNoCrash1`–`3`, `parseUnaryExpressionNoTypeAssertionInJsx1`–`3`, `jsxAndTypeAssertion`, `jsxCheckJsxNoTypeArgumentsAllowed`, `tsxAttributeInvalidNames`, `checkJsxNotSetError` |
| TS7006/7005/7034 in and around attributes | contextual typing (contextual.rs, main) | `jsxChildrenIndividualErrorElaborations` 38:4, `jsxFragmentFactoryNoUnusedLocals`, `tsxAttributeResolution2`, `tsxInArrowFunction`, `tsxReactEmitNesting`, `tsxStatelessFunctionComponents1` 40:29, `tsxEmit1`, `tsxReactEmit1` |
| TS2304 on the tag or spread | factory-name resolution | `jsxAttributeWithoutExpressionReact`, `jsxSpreadTag` (two keys), `parseJsxExtends2`, `jsxElementType` 98:2/99:2 |
| Attribute relation (TS2322/2741/2559/2345 missing) | relation report, alias targets (`r5-jsx3.md` §6) | `tsxLibraryManagedAttributes`, `tsxStatelessFunctionComponents1`, `tsxSpreadAttributesResolution5`/`6`, `checkJsxChildrenProperty4`, `jsxClassAttributeResolution`, `jsxExcessPropsAndAssignability`, `jsxCallElaborationCheckNoCrash1`, `ignoredJsxAttributes`, `jsxFragmentWrongType`, `quickIntersectionCheckCorrectlyCachesErrors`, `excessiveStackDepthFlatArray` |
| Factory and container checks | `jsx_factory.rs`, `getJsxElementPropertiesName` | `jsxFragmentFactoryReference(jsx=react)`, `inlineJsxFactoryWithFragmentIsError` (TS2879), `inlineJsxFactoryDeclarationsLocalTypes` (TS2609), `tsxSpreadAttributesResolution17` (TS2607), `tsxElementResolution15` (TS2608), `jsxIssuesErrorWhenTagExpectsTooManyArguments` (TS6229) |
| Others | various | `jsxNamespaceGlobalReexport`, `jsxNamespaceImplicitImportJSXNamespace` (extra TS7026), `jsxElementTypeLiteralWithGeneric`, `jsxElementType` 91:2 (TS2786), `tsxStatelessFunctionComponents2` (TS2551/2339), `multiline` |

## 1. JSX overload resolution (`chooseOverload` for a value tag)

**Forcing constraint.** 14 cases miss TS2769 on a JSX tag. The common
shape is not exotic: `React.Component` declares two constructors
(`(props: Readonly<P>)` and the deprecated `(props: P, context?: any)`), so
every React class component is an overload set, and every failing
attributes check on one is `No overload matches this call.` The port's
resolver (`resolve_jsx_attributes_context`, `jsx_intrinsic.rs`) answers only
a one-candidate list; with two it published nothing and the attributes
check (`check_jsx_attributes_assignable`) declined.

**What is ported** (`jsx_component.rs`, `choose_jsx_overload`), following
`resolveCall` (`checker.go:8843`):

- the candidates are `getUninstantiatedJsxSignaturesOfType`'s list
  (construct signatures, else call signatures) through
  `reorderCandidates`;
- `chooseOverload` (`checker.go:9025`) runs under `subtypeRelation` and then
  `assignableRelation`, resetting `candidatesForArgumentError` per pass;
- each candidate is filtered by `hasCorrectTypeArgumentArity` and the JSX
  arm of `hasCorrectArity` (`checker.go:9136`). With an attributes argument
  (`getEffectiveCallArguments`: properties, or children on an opening
  element) every candidate's parameter count is **one**, so a parameterless
  overload is an argument-error candidate whose props are
  `IntrinsicAttributes & unknown` — the oracle confirms TS2769 for
  `F(): Element; F(p: {a: string}): Element` with `<F a={1} />`;
- a generic candidate is instantiated by its written type arguments
  (constraint violations make it `candidateForTypeArgumentError`) or by
  inference from the attributes, as the single-candidate resolver infers;
- applicability is `checkApplicableSignatureForJsxCallLikeElement`
  (`jsx.go:590`): the attributes type, checked with the candidate's
  `getEffectiveFirstArgumentForJsxSignature` as contextual type, related to
  it — including the fresh-literal excess check that runs before the
  structural relation (`r5-jsx3.md` §3), factored out of the existing
  report path as `jsx_attributes_relation` so the choice and the report
  decide identically.

The chosen candidate is published in `resolved_call_signatures` in the
single-candidate resolver's shape (one `props` parameter, no type
parameters), so the `ElementClass` bound and later contextual reads see it.
A failure reports through the existing attributes report against the last
argument-error candidate (`reportCallResolutionErrors`, `checker.go:9649`),
under that candidate's context; with more than one such candidate the head
is rewritten to TS2769, as the call road does
(`check_overload_candidates_arguments`, `calls.rs`). The `The last overload
gave the following error.` chain and the related information wait on
`tsr-2zk.22`, as on the call road.

**Contextual type per candidate.** Native checks the attributes once per
candidate with that candidate's props as contextual type
(`checkExpressionWithContextualType`). The port has one answer per element
for `jsx_attributes_context`: the active inference context, then the
published signature. `with_jsx_candidate_context` makes the candidate the
element's active context while its attributes type is built, which is
exactly what the single-candidate resolver already does for a generic
candidate. Literal widening asks the contextual type after the expression
cache, so a literal attribute keeps its literal type against each
candidate; a context-sensitive attribute or child does not (its first
checked type is cached), so that shape declines (below).

**Declined, each with its reason:**

- context-sensitive attributes or children: `argCheckMode`
  `SkipContextSensitive` re-checks them per candidate, which this port's
  expression cache cannot do (`tsxStatelessFunctionComponentOverload5`
  50:24 is this shape);
- a union or `any` tag, an unresolved implicit-runtime namespace
  (`r5-jsx3.md` §3), an element already under an active context;
- a failure with no argument-error candidate: the arity and type-argument
  reports (`getArgumentArityError`, `checkTypeArguments` with reports) are
  not ported on this road;
- `ElementAttributesProperty` with several members (TS2608) or an
  unenumerable container: no props are built.

**Not a new cache.** No table is added. The only write is the existing
`resolved_call_signatures` entry for the element, which the single-candidate
resolver already owns; it is keyed by the opening-like `NodeId`, written
once after a candidate is chosen and never for a failed set. The work per
element is one attributes build and one relation per candidate per pass —
native's own cost — and it runs only for JSX elements whose tag has
several signatures.

**Measured** against the frozen base (both dumps unfiltered; row-level diff
of every case): +27 rows fixed, no new missing or extra row anywhere; WRONG
→ RIGHT `spellingSuggestionJSXAttribute`,
`tsxStatelessFunctionComponentOverload4`; rows fixed in still-WRONG cases:
`jsxElementType` 70:2, 72:20; `tsxNotUsingApparentTypeOfSFC` 15:14, 18:14;
`contextuallyTypedStringLiteralsInJsxAttributes02` 31:56, 33:43, 34:36;
`tsxElementResolution9` 11:2, 18:2, 25:2;
`tsxStatelessFunctionComponentOverload5` 56:51, 57:68, 58:13;
`tsxStatelessFunctionComponentsWithTypeArguments4` 11:15. Type lines
byte-identical. Coverage: `diagnostics` 4660/5502, `checker_types`
8498/9538. slowcases clean on both dumps. Median child CPU against the
frozen binary (21 samples): domain-model 1.002, generic-imports 0.964
(neither project has JSX).

**Falsifier.** `crates/tsr-checker/tests/r6_jsx.rs` pins four oracle
answers (TS2769 positions, the silent choice, the class pair, and the
parameterless candidate). A JSX overload set where TSR reports TS2769 and
the oracle chooses a candidate would show the subtype pass or the
per-candidate contextual type is wrong.
