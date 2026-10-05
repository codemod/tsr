# Lane `implicit-any-widening` notes (tsr-2zk.11)

`noImplicitAny` reporting (`reportImplicitAny`, `reportErrorsFromWidening`) and
literal widening. Baseline for every number below: the `06f25e0` lane list,
measured from branch head `7070635` with `diagverdictdump` (3,471 RIGHT
diagnostics cases, 2,017 WRONG).

## 1. TS7051 is an arm of `reportImplicitAny`, not a separate rule

`reportImplicitAny`'s parameter arm (`checker.go:18290`, pinned `5b1047d`)
reports `Parameter has a name but no type. Did you mean 'arg{N}: {name}'?`
instead of TS7006/TS7019 when the parameter belongs to a call signature, a
method signature or a function type **and** its name is either a type keyword
(`IsTypeNodeKind(IdentifierToKeywordKind(name))`) or resolves with the `Type`
meaning from the parameter. `implicit_any.rs` printed TS7006/TS7019 for these
because the arm did not exist.

`parameter_name_is_probably_a_type` asks exactly those two questions:
`tsr_scanner::keyword_kind` for the keyword half (only the keyword members of
`IsTypeNodeKind` can come out of a keyword lookup, so the type-node range and
JSDoc kinds are unreachable here) and `Binder::resolve_name(…, TYPE)` for the
resolution half. Construct signatures and constructor types are excluded
because upstream's kind test names only the three kinds.

**Measured.** `noImplicitAnyNamelessParameter`, `strictModeReservedWord2`
converted; corpus TS7051 missing 10 → 0, no verdict lost.

## 2. No `file_has_parse_errors` gate on implicit-any reporting

Every rule in `implicit_any.rs` opened with `|| self.file_has_parse_errors`.
Upstream has no such gate: `reportImplicitAny` runs from the widening of a
declaration's type whatever the file's syntactic diagnostics, and tsgo's
baselines carry TS7006/TS7031/TS7010 beside TS1005 (`destructuringParameterDeclaration2`
reports 35 binding-element lines after a missing `]` on line 8).

**Measured** (whole corpus, line-level): missing 7,151 → 7,045, extra
1,794 → 1,813; eleven cases converted, no RIGHT/EMPTY_RIGHT verdict lost, no
types line lost.

**The 19 new extra lines are parser divergences, not this rule.** Each sits in
a file where TSR's recovery tree differs from tsgo's, so the declaration this
rule sees is not the declaration upstream sees:

| case | TSR tree | tsgo tree |
|---|---|---|
| `parseInvalidNullableTypes`, `parseInvalidNonNullableTypes` | `a: string?` / `a: ?string` loses the annotation (TS7006) | `JSDocNullableType` / `JSDocNonNullableType` annotation |
| `derivedClassSuperCallsInNonConstructorMembers`, `reservedWords2/3`, `MemberFunctionDeclaration5_es6`, `parserErrorRecovery_ParameterList6`, `parserEqualsGreaterThanAfterFunction1` | a recovered, name-bearing bodiless method/function (TS7010) | a different recovery shape |
| `parametersSyntaxErrorNoCrash1/2/3`, `importCallExpressionIncorrect2` | a binding pattern recovered as a parameter (TS7031) | a different recovery shape |
| `ArrowFunction3`, `parserX_ArrowFunction3` | `(a): =>` recovered as an arrow with an unannotated `a` | `a` is not reported |

These are owned by the parser crates and reported to the integrator rather
than suppressed here; a gate that hides them also hid 106 correct lines.

**Falsifier.** If a parser-recovery fix lands and one of these cases still
reports the extra line, the cause is in this rule, not the tree.

## 3. TS7006 on a context-sensitive function asks the context, not the syntax

`getTypeForVariableLikeDeclaration` (`checker.go:16652`) answers nil for an
unannotated, initializer-less parameter exactly when
`getContextuallyTypedParameterType` (`checker.go:29458`) does, and only then
does `widenTypeForVariableLikeDeclaration` report the implicit `any`. That
function answers nil outright for anything but a function expression, an arrow
or an object-literal method — so a function declaration, a class method, a
constructor and every signature are "uncontextual" by upstream's own test, not
by an allow-list.

For the three context-sensitive forms the rule used a syntactic allow-list of
parent positions (an unannotated variable initializer, an expression
statement, a property initializer, a `return` in an unannotated function).
`contextual_parameter_type_is_absent` replaces it with upstream's three nil
sources, each asked of the data upstream reads:

1. **No contextual type at all** — `has_no_contextual_type`, the walk over
   `getContextualType`'s nil-answering arms. Asked *before* the port's
   contextual parameter lookup, in upstream's order: with no contextual type
   there is no contextual signature, whatever `crate::contextual` answers.
2. **No usable signature** (`ContextualSignature::Absent`) — **not trusted.**
   Admitting it lost twelve RIGHT/EMPTY_RIGHT cases
   (`contextualTypeCaching`, `contextuallyTypedByDiscriminableUnion`,
   `discriminantPropertyInference`, `discriminantUsingEvaluatableTemplateExpression`,
   `genericInferenceDefaultTypeParameter`, `inferenceContextualReturnTypeUnion1`,
   `inferentialTypingUsingApparentType1`, `inferredReturnTypeIncorrectReuse1`,
   `inferringAnyFunctionType4`, `intersectionOfTypeVariableHasApparentSignatures`,
   `returnTypeInferenceContextualTypeIgnoreAnyUnknown1`,
   `typeInferenceCacheInvalidation`, `parserArgumentList1`): `crate::contextual`
   answers `Absent` where tsgo finds a signature (discriminated-union
   contextual types and inference-context instantiation dominate). A producer
   defect reported to the integrator, not worked around here.
3. **A present signature that is too short** — `tryGetTypeAtPosition`
   (`signature_type_at_position`) answers `None` past the last parameter
   without a rest. Admitting it moved no verdict either way.

The IIFE arm (`checker.go:29463`) runs first upstream, so an IIFE's
parameters are never reported from these sources.

### Two retained declines, each owed to another file

- **`returned_from_an_iife`.** `getContextualReturnType` (`checker.go:29665`)
  gives a `return` inside an IIFE the IIFE call's own context.
  `has_no_contextual_type`'s `ReturnStatement` arm (`signatures.rs`) lacks that
  arm and answers "no context" for `contextualReturnTypeOfIIFE3`'s
  `return { someFun(arg) {} }`, which lost that case. The proof is declined
  under any return of an IIFE until the arm is ported there.
- **`retained_return_position_report`** — the old allow-list's `return` arm,
  kept verbatim. In `subtypeReductionWithAnyFunctionType` tsgo has no
  contextual signature for `x => x.length > 0` (returned from the argument of
  `useMemo<T>(func: () => T)`), but `get_contextually_typed_parameter_type`
  answers `any` (tsr-2zk.31). Dropping the arm loses that RIGHT case; keeping
  it keeps TS7006 extra wherever a returned function's contextual type comes
  from the owner's contextual signature (`asyncFunctionContextuallyTypedReturns`,
  `contextualTypeOnYield2`, `inferPropertyWithContextSensitiveReturnStatement`,
  `invalidThisEmitInContextualObjectLiteral`). Remove it once that producer
  answers nil.

**Measured** against §2's head: RIGHT 3,482 → 3,487, EMPTY_RIGHT 4,914 → 4,915,
no verdict lost, no types line lost. Corpus TS7006 missing 93 → 63, extra
27 → 22 (lane: missing 71 → 41). One new extra line,
`intraBindingPatternReferences` (18,54), in an already-WRONG case:
`has_no_contextual_type`'s `VariableDeclaration` arm ignores that a
binding-pattern name supplies a contextual type (`checker.go:29431`) —
`signatures.rs`, reported.

**Falsifier.** When `crate::contextual` stops answering `Absent` where tsgo
has a signature, admitting source 2 must convert cases without losses; if it
still loses, the cause is here.

## 4. TS7008 follows `getFlowTypeInConstructor`'s reference matching

An unannotated, initializer-less property is typed from
`getFlowTypeInConstructor` / `getFlowTypeInStaticBlocks` (`flow.go:2466`,
`:2488`) and reports TS7008 only when that answers nil. The flow query reads a
synthesized `this.<name>`, which `isMatchingReference` matches against
`this.#name` and a literal element access `this['name']` as well; and a
**static** member reads the class's static blocks, not the constructor.
`subtree_assigns_this_member` (`check.rs`, used only by this rule) still
approximates the flow query by "an assignment exists in the body"; it now
matches the same reference forms, and the member rule asks static blocks for
static members. `isPrivateWithinAmbient`'s private-identifier half is added to
the ambient exemption (`declare class A { #prop; }`).

**Measured.** Corpus TS7008 extra 16 → 1, missing 1 → 0; seven cases
converted, no verdict lost, no types line lost.

**Known approximation.** Upstream also answers nil (and reports) when every
assigned value is nullable (`everyType(flowType, IsNullableType)`); the
syntactic test does not see types and declines there.
