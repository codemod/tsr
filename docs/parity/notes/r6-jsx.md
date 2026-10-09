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

## 2. TS2879: the fragment factory is not in scope (`getJSXFragmentType`)

**Forcing constraint.** `resolveJsxOpeningLikeElement` types an opening
fragment through `getJSXFragmentType` (`jsx.go:499-522`), which resolves the
fragment factory (`getJsxNamespace` at the fragment) as a value with TS2879,
`Using JSX fragments requires fragment factory '{0}' to be in scope, but it
could not be found.`, as the not-found message. The port typed no fragment,
so the line was missing beside TS2874 in `jsxFragmentFactoryReference`
(`jsx=react`) and `inlineJsxFactoryWithFragmentIsError` (`index.tsx`, an
`@jsx dom` pragma with no `@jsxFrag`: the fragment factory falls back to
`React`).

**What is ported** (`jsx_factory.rs`, `check_jsx_fragment_factory_in_scope`,
dispatched from the existing per-node `check_jsx_fragment_factory` on a
`JsxOpeningFragment`): the guard `(jsx == react || jsxFragmentFactory set)
&& name != "null"`, the implicit-import container first, the value lookup
(Enum excluded under `preserve`/`react-native`) through the same
`onFailedToResolveSymbol` arms `markJsxAliasReferenced` uses
(`resolve_jsx_factory_name`, now taking the not-found message).

**Once per file.** Upstream caches the answer in `sourceFileLinks.jsxFragmentType`,
so only the first fragment it types can report. Asked from each fragment,
the first opening fragment of a pre-order walk stands for that first
request — the same substitution `r5-jsx3.md` §7 made for TS2875's
`firstJSXTagInFile`, and with the same cost shape: the walk enters only
subtrees that start no later than the fragment, and only runs when the
factory guard holds. Where the first-typed fragment is not the first in
source order (a fragment typed early through contextual typing elsewhere)
the two could differ; no corpus case has two fragments under different
scopes for the factory name.

**Not ported:** the fragment's type (`React.Fragment`'s `typeof` and its
signatures, so `resolveCall` on a fragment and TS2604 on one).

**Measured** against the frozen base, on top of §1: WRONG → RIGHT
`jsxFragmentFactoryReference(jsx=react)`, `inlineJsxFactoryWithFragmentIsError`;
no new missing or extra row in any case; type lines byte-identical;
slowcases clean on both dumps.

## 3. Type arguments on a JSX tag (TS2558, TS2344)

**Forcing constraint.** 29 rows in three cases: `jsxIntrinsicElementsTypeArgumentErrors`
(10), `jsxUnclosedParserRecovery` (13), `tsxTypeArgumentResolution` (6).

- *Intrinsic tag.* `resolveJsxOpeningLikeElement`'s intrinsic arm
  (`jsx.go:552-558`) checks written type arguments as source elements and
  reports `Expected 0 type arguments, but got {n}.` over the list,
  whatever the attributes. Ported as `check_jsx_intrinsic_type_arguments`,
  run before the intrinsic attributes check.
- *Value tag, no candidate of the written arity.* `chooseOverload` skips
  every candidate on `hasCorrectTypeArgumentArity` and records nothing, so
  `reportCallResolutionErrors` falls to its last arm and
  `getTypeArgumentArityError` (`checker.go:9853`) reports over the whole
  list (one signature: its `min-max`; several: the nearest count below or
  above, TS2743 with both). This answer needs no relation, so
  `choose_jsx_overload` decides it first — before the context-sensitive
  decline — and for a single candidate too (`isSingleNonGenericCandidate`
  with written type arguments returns before recording anything, which is
  the same arm).
- *A constraint violation.* `checkTypeArguments` without reports makes a
  candidate `candidateForTypeArgumentError`; with no argument-error
  candidate, `checkTypeArguments` with reports puts TS2344 on the first
  written argument outside its instantiated constraint. Written arguments
  are completed by `fillMissingTypeArguments` (a default instantiated over
  the arguments before it, else `unknown`), which the chooser now does
  instead of declining a short list (`<MyComp2<Prop>>` with `P2 = {}`).
  A single generic candidate with written type arguments now goes through
  the chooser as well (one assignable pass; the subtype pass only runs for
  several candidates, as `resolveCall` does).

**Span.** `NewDiagnosticForNodeList` runs from the first argument to the
list's end, which includes a trailing comma; this port's span ends at the
last argument (the start, which the suite compares, is the same), as the
call road's `report_type_argument_arity_error` does.

**Not ported, with the owner:** TS1099 (`<div<>>`, an empty list) and
TS1009 (a trailing comma) from `checkGrammarJsxElement`'s
`checkGrammarTypeArguments`. The parser keeps neither an empty JSX
type-argument list nor its trailing comma (an empty list is `&[]`, the same
as no list), so the grammar check cannot ask; the parser (main) would need
to record both. 6 rows (`jsxIntrinsicElementsTypeArgumentErrors` 5:15,
7:22, 18:15, 20:22; `tsxTypeArgumentResolution` 26:12, 28:12).

**Measured** on top of §1–§2: +29 rows, no new missing or extra row in any
case, type lines byte-identical, slowcases clean. Oracle-checked fixtures
in `tests/r6_jsx.rs`.

## 4. TS2608: an `ElementAttributesProperty` with several members

**Forcing constraint.** `getNameFromJsxElementAttributesContainer`
(`jsx.go:1093`) reports TS2608, `The global type 'JSX.{0}' may not have
more than one property.`, on the container's first declaration when it has
more than one member, and answers `InternalSymbolNameMissing` (the props
are then the first parameter). `getJsxPropsTypeFromClassType` asks it for
every component reference, so any class-component element in a program
with such a container reports it; `tsxElementResolution15` missed it.

**What is ported.** `jsx_element_properties_name` reports and answers
`Missing` instead of declining, and the component check asks it for every
element whose tag has construct signatures
(`check_jsx_element_properties_container`). Upstream reports once per
call and the program's `SortAndDeduplicateDiagnostics` keeps one; the port
reports once per position (a scan of the checker's diagnostics on this
path only, which runs only for a malformed container).

**Measured** on top of §1–§3: WRONG → RIGHT `tsxElementResolution15`, no
new missing or extra row, type lines byte-identical, slowcases clean.

## 5. The return-type bound after a failed overload set (TS2786)

**Forcing constraint.** `resolveCall` answers a failed set with
`getCandidateForOverloadFailure`; with several non-generic candidates that
is `createUnionOfSignaturesForOverloadFailure` (`checker.go:9581`), whose
return type is the **intersection** of every candidate's return type, and
`checkJsxOpeningLikeElementOrOpeningFragment` relates that return type to
the `JSX.Element | null` / `ElementClass` bound like any resolved
signature's. `tsxElementResolution9` reports TS2786 with `Its return type
'{ x: number; } & { y: string; }'`; the port published nothing for a failed
set and skipped the bound.

**What is ported.** `JsxOverloads::Failed` carries
`jsx_overload_failure_return`: the intersection over all (reordered)
candidates when none is generic, the candidate itself for one, `None`
otherwise (`pickLongestCandidateSignature` among generic candidates is not
ported). The component check relates it to the bound in place of a
published signature. The signature itself is still not published (its
combined parameters would be the attributes' contextual type).

**Measured** on top of §1–§4: WRONG → RIGHT `tsxElementResolution9`, no new
missing or extra row, type lines byte-identical, slowcases clean.

## 9. Diffs for files this lane does not own

Each diff is against the frozen base plus this lane's commits, measured
alone on top of them (both dumps unfiltered, row-level diff of every case).

### D1. Value tag names are value references (`check.rs`, main)

`r6-jsx-tag-name-value-reference.diff`. `resolveJsxOpeningLikeElement`
checks a non-intrinsic tag name as an expression (`jsx.go:562`) and
`checkJsxElementDeferred` checks the closing tag's name the same way
(`jsx.go:82`), so an unresolved `<Comp />` reports TS2304 through
`getResolvedSymbol` like any identifier expression. TS2304 here is keyed on
`is_value_reference`'s allow-list, which had no arm for a tag name, so
`<View>…</View>` with nothing named `View` was silent. The diff adds the
three tag positions (opening, self-closing, closing), excluding intrinsic
names (`isIntrinsicJsxName`: a lowercase first letter or a hyphen), which
resolve through `JSX.IntrinsicElements` instead. A dotted tag
(`<a.B />`) was already covered by the property-access arm.

Measured: WRONG → RIGHT `jsxAttributeWithoutExpressionReact`,
`jsxSpreadTag(target=es2015)`, `jsxSpreadTag(target=esnext)`,
`parseJsxExtends2`; rows fixed in `jsxElementType` (98:2, 99:2) and
`jsxUnclosedParserRecovery` (95:5); +17 rows, no new missing or extra row,
type lines byte-identical, slowcases clean, `tsr-checker` tests pass.

### D2. The attributes resolver asks the chooser (`jsx_intrinsic.rs`, r6-errorsplit2)

`r6-jsx-resolver-chooses-overloads.diff`. `resolve_jsx_attributes_context`
answers the contextual type of every attribute and child; with several
candidates it answered nothing, so attributes of an overloaded component
were checked with no contextual type even when §1's chooser later picked
a candidate. Native has one resolution (`getResolvedSignature` caches
`resolveCall`'s answer), and the attributes are contextually typed by the
chosen candidate. The diff makes the resolver ask
[`jsx_overloads_at`](../../../crates/tsr-checker/src/jsx_component.rs) for
a list of more than one and read the signature it published; a failed or
declined set still has no context (native would use
`getCandidateForOverloadFailure`'s pick — not ported). The chooser runs at
most twice per failing element (resolver, then the component check), and
once per chosen one (the second asker finds the published signature).

Measured on top of §1–§3 (lane commit `jsx_overloads_at` included):
diagnostics byte-identical; type lines +5 RIGHT, 0 lost
(`contextuallyTypedStringLiteralsInJsxAttributes02` 0:36–0:39, 0:42:
literal attributes of an overloaded component keep their literal type).
