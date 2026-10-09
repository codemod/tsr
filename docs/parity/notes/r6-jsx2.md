# r6-jsx2 — JSX diagnostics, continued (`tsr-2zk.1147`)

Round-6 lane notes, continuing `r6-jsx.md` (§7 is this lane's board) and
`r5-jsx3.md`. Native source is `vendor/typescript-go` @ `5b1047d`,
`internal/checker/jsx.go` unless noted. The lane owns `jsx_attributes.rs`,
`jsx_component.rs` and `jsx_factory.rs`; everything else is shipped as a
measured diff (§9 lists them in apply order).

## 0. The frozen base

Batch BQ (r6-jsx) had not landed when this lane started, so §1 and §2 were
first measured on what BQ would land: main `5e4d21b` merged with r6-jsx's
lane tip `8d5fa73` plus its two held diffs (D1 `check.rs`, D2
`jsx_intrinsic.rs`). BQ then landed (`2e26f22`, D1 and D2 included); the
lane merged it and **re-froze there**, and §1–§2 were re-measured against
`2e26f22` with the same transitions. Every number from here on, and every
diff in §9, is against `2e26f22`.

| | diagnostics RIGHT | EMPTY_RIGHT | type lines RIGHT |
|---|---|---|---|
| frozen base `2e26f22` | 5637 | 5606 | 550,360 |
| §1–§2 | 5639 | 5606 | 550,368 |

## 1. Context-sensitive attributes over an overload set

**Forcing constraint.** `r6-jsx.md` §1's chooser declined whenever an
attribute or child was context-sensitive (an arrow with unannotated
parameters, say), because the port caches an expression's first checked
type and native re-checks such attributes per candidate. That shape is
common: `<MainButton onClick={(k) => log(k)} extra />` over two overloads
missed TS2769 (`contextuallyTypedStringLiteralsInJsxAttributes02` 32:24).

**What native does.** `resolveCall` sets `argCheckMode` to
`SkipContextSensitive` when any argument is context-sensitive
(`checker.go`, before `chooseOverload`). `chooseOverload`
(`checker.go:9040-9103`) then checks each candidate twice while that mode
holds: first with the attributes built as `SkipContextSensitive`
(`checkExpressionWithContextualType(attributes, paramType, nil, checkMode)`
in `checkApplicableSignatureForJsxCallLikeElement`, `jsx.go:599`), where a
context-sensitive function is `anyFunctionType`, and related as
`getRegularTypeOfObjectLiteral(attributesType)` (`jsx.go:678`) — so **no
excess-property check runs**; then, once a candidate passes, `argCheckMode`
becomes `Normal` for good, a generic candidate re-infers in the same
inference context (`checker.go:9086`), and the ordinary (fresh) check
decides. A candidate failing either check is an argument-error candidate.

**What is ported** (`jsx_component.rs`):

- `choose_jsx_overload` keeps `skip_context_sensitive` in place of the
  decline, cleared by the first candidate that passes the skip check.
- `jsx_skip_context_sensitive_candidate` is that first check: a
  non-generic or explicitly instantiated candidate is
  `jsx_check_candidate`'s; a generic one infers from the skip image
  (`inferJsxTypeArguments`, `jsx.go:197`) and is instantiated; the skip
  image, under the instantiated candidate as context, is related to the
  effective first argument with `fresh: false`.
- `jsx_attributes_relation` takes `fresh`: without it the relation is the
  structural one alone (`JsxExcess::None`).
- `jsx_check_candidate` takes the skip pass's inferences as a `seed`, so
  the `Normal` inference continues from them, as native's shared
  `InferenceContext` does.

The skip image is `jsx_attributes_inference_type(_, true)` — the same
builder the single-candidate resolver infers from — which is private to
`jsx_intrinsic.rs` (r6-errorsplit2's file): diff **J1** makes it
`pub(crate)`.

**Caveat in the skip image, not changed here.** The builder's skip arm
asks `context_free_function_type` for *every* arrow or function expression
before testing context sensitivity, so an arrow with annotated parameters
(not context-sensitive natively, hence checked in full) becomes a
parameterless return-only signature. In an applicability check this can
only accept more than native, and the `Normal` check that follows still
decides; it can move the point where `argCheckMode` turns `Normal` one
candidate early. No corpus case was seen to depend on it; the fix belongs
to the builder (`jsx_intrinsic.rs`).

**Measured** against the frozen base (both dumps unfiltered, row-level
diff of every case): WRONG → RIGHT
`contextuallyTypedStringLiteralsInJsxAttributes02`; type lines +8 RIGHT
(`checkJsxSubtleSkipContextSensitiveBug` 0:22–0:25,
`contextuallyTypedStringLiteralsInJsxAttributes02` 0:47–0:49, 0:52), 0
lost; no other case's rows changed; slowcases clean on both dumps. Median
child CPU against the frozen binary (21 samples): domain-model 1.013,
generic-imports 1.013 (neither project has JSX; the path only runs for an
overloaded JSX tag). Oracle-checked fixture:
`crates/tsr-checker/tests/r6_jsx2.rs`
(`context_sensitive_attributes_choose_among_overloads`).

**Not converted, with the blocker each was traced to:**

- `reactDefaultPropsInferenceSuccess` ×3: the candidates are generic class
  constructors whose props go through `LibraryManagedAttributes<typeof
  FieldFeedback, P>`; the port leaves that conditional deferred over an
  unsubstituted `C` (`C extends { defaultProps: infer D } ? Defaultize<P, D>
  : P`), so the skip inference finds nothing and `instantiate_signature`
  answers `None` on it. Native resolves the conditional (the tag type is
  not generic) to `Defaultize<P, …>`. This is the managed-attributes alias
  target the round leaves alone (`r5-jsx3.md` §6; `declared.rs`).
- `tsxStatelessFunctionComponentOverload5` 50:24: the skip check of the
  first candidate relates `anyFunctionType` to `React.MouseEventHandler<any>`
  (an alias of the call-signature interface `EventHandler<E>`), which the
  relater answers `Unknown` at its no-members-table fall-through
  (`relater.rs`, `reasons::Site::NoMembersTable`); native relates it
  (`signaturesRelatedTo`'s `anyFunctionType` wildcard after empty
  `propertiesRelatedTo`). The same alias-reference producer as above.

**Considered and set aside: `ObjectFlagsObjectLiteral` on the minted
attributes.** Native's attributes object is an object literal
(`jsx.go:725`), so the subtype pass does not require the target's optional
members (`requireOptionalProperties`). The port's minted chunk is not in
`object_literal_spread_flags`, so under `Subtype` a missing optional
`key`/`className` fails the candidate and the assignable pass decides
instead. Registering it changes no row of the cases above (both passes
reach the same verdict there) and touches every reader of that table
(widening, union property reads, discriminant reports), so it is not
shipped with this item; it is a candidate for the builder's owner.

## 2. TS6229: the tag needs more arguments than the factory provides

**Forcing constraint.** `checkApplicableSignatureForJsxCallLikeElement`
(`jsx.go:602-677`) runs `checkTagNameDoesNotExpectTooManyArguments` between
building the attributes type and relating it: a tag whose call signatures
all need more arguments than any call signature of the factory's first
parameter accepts reports TS6229, `Tag '{0}' expects at least '{1}'
arguments, but the JSX factory '{2}' provides at most '{3}'.`, on the tag
name, and fails the candidate. `jsxIssuesErrorWhenTagExpectsTooManyArguments`
missed both rows.

**Why the old decline was right, and why a replacement lost 30 cases.**
`check_jsx_attributes_assignable` skipped the attributes relation for any
tag one of whose call signatures needs more than one argument. That gate is
exactly where TS6229 can fire: the factory's first-parameter signatures
take at least the props (`maxParamCount >= 1` whenever
`hasFirstParamSignatures` holds), so a tag whose every minimum is at most
one always fits. r6-jsx's draft replaced the gate with the full check and
declined whenever a factory first parameter's signature list was unread —
which, with `React.SFC<P>` unread, was every React element: 30 cases' rows
lost. So the port keeps the gate and runs the check **only inside it**: a
decided fit continues to the relation, a decided failure reports TS6229, and
anything undecided declines as before. Nothing outside the old decline
changes.

**What is ported.**

- `jsx_tag_argument_count_fits` (`jsx_component.rs`): the automatic
  runtime fits; the factory entity is resolved as a value; each factory
  call signature's `getTypeAtPosition(sig, 0)` is read for call
  signatures; a rest parameter fits, else the largest parameter count is
  compared with the smallest `getMinArgumentCount` among the tag's call
  signatures. No factory symbol, no factory signature, no first parameter
  with signatures all fit, as upstream answers `true`.
- TS6229 is reported for a tag with **one** call signature (the
  single-candidate road: `reportCallResolutionErrors` reports the
  candidate's applicability error). With several, native's failure is
  TS2769 with a chain; that stays declined.
- The factory entity (`getJsxFactoryEntity`, `jsx.go:1406`):
  `jsx_factory_entity_text` answers the option-derived entity and declines
  for a file with an `@jsx` pragma, whose full text the host does not keep
  (only its first identifier). `resolve_jsx_factory_entity`
  (`jsx_factory.rs`) is `resolveEntityName(…, Value, ignoreErrors,
  dontResolveAlias: false)` over that dotted text: the root at the tag, then
  each member through `getExportsOfSymbol` (a module past its `export =`).
  Its export lookup repeats `declared.rs`'s private `get_symbol_of_exports`
  (that function takes an AST entity; this one has text).
- The related information (`'{0}' is declared here.`) is not attached: no
  checker report attaches related information yet, and it is not compared.

**The two pieces outside this lane.**

- *The entity text* (diff **J2**, `r6-jsx2-factory-entity-text.diff`,
  `checker.rs`): `c._jsxFactoryEntity` as text — `jsxFactory` when it
  parses as an entity name, else `{jsx_namespace}.createElement`
  (`jsx.go:1373-1386`). The checker kept only the first identifier.
- *`React.SFC<P>`'s signatures.* `SFC<P>` is a generic alias reference,
  which this port mints with its own identity (`type_reference_targets`)
  where native's reference is the instantiated body; its signature list is
  `None` (`signature_candidates_of_named_type`, since `r4-jsx.md` §2). The
  producer fix — read the body's signatures for an alias member table —
  was measured as `r6-jsx2-alias-reference-signatures.diff` (`signatures.rs`):
  it converts this case too, but `coAndContraVariantInferences6` gains a
  false TS2345 (34:23) beside its missing TS2322 (34:42): with
  `JSXElementConstructor<ExactProps>`'s signatures readable, the call road
  reaches a report whose elaboration is not ported. **Held, not for
  apply**, with that row as its blocker. The lane instead projects the body
  where it reads the factory's parameter (`binding_type_alias_body`, the
  idiom `destructure.rs` and the JSX resolver already use for generic alias
  identities), and a union whose constituent has no call signatures answers
  none (`getUnionSignatures`, `checker.go:21117`) — which decides `SFC<P> |
  ComponentClass<P> | string` without the union's own list.

**Measured** on top of §1 (both dumps unfiltered, row-level diff of every
case): WRONG → RIGHT `jsxIssuesErrorWhenTagExpectsTooManyArguments`; no
other case's rows changed; type lines byte-identical; slowcases clean.
Message text checked against the oracle on the case's own source. Median
child CPU against the frozen binary (21 samples): domain-model 0.966,
generic-imports 0.989. Oracle-checked fixture:
`a_tag_needing_more_arguments_than_the_factory_provides_is_ts6229`.

## 3. TS2607 on a class extending `any` — routed, two producers

`tsxSpreadAttributesResolution17`: `class Empty extends React.Component<{}, {}>`
with `declare var React: any`. Native: `getDefaultConstructSignatures`
(`checker.go:20857`) finds no construct signature on the `any` base
constructor (`getSignaturesOfType(any, …)` is empty: `any` is not
structured), so `typeof Empty` has one default signature returning `Empty`;
`resolveBaseTypesOfClass` makes the base type `any`, which contributes no
members (only an `any` string index), so `getPropertyOfType(Empty,
"props")` is nil and `getJsxPropsTypeFromClassType` reports TS2607.

Two producers stand between the port and that answer, neither in this lane:

1. `resolve_class_construct_signatures` (`signatures.rs`, r6-printer4)
   asks `signatures_of_type_kind(any, Construct)`, which is `None`, so the
   whole list is `None`. Diff **J3** (`r6-jsx2-any-base-default-construct.diff`)
   treats an `any` base like `null`: empty, then the default signature.
   Measured alone on top of §1–§2: **no transition** on either dump (no
   corpus case reads it without the second piece). Faithful; ship with 2.
2. `get_property_names_of_type(Empty)` (`members.rs`, main) is `None`: the
   heritage walk (`base_symbol_of_heritage_entry`) cannot name a base symbol
   for an expression of type `any` and declines the whole table. Native's
   names are the class's own members. The fix is an `any` base constructor
   arm in the class names walk (and the `any` index info beside it); not
   attempted here, since property-name completeness feeds the excess and
   relation checks everywhere.

`check_jsx_class_attributes_member` needs no change once both land.

## 4. TS2786 with `JSX.ElementType` defaults — routed, `declared.rs`

`jsxElementTypeLiteralWithGeneric` (and `jsxElementType` 91:2). Native's
`getJsxElementTypeTypeAt` is `getJsxType(ElementType)`, i.e.
`getDeclaredTypeOfSymbol` of the alias: for a **generic** alias that is its
body with the type parameter `P` free — not instantiated with `P`'s
default. `"ruhroh"` is then not assignable to the mapped/indexed-access arm
(`P extends IntrinsicElements[K] ? K : never` over a free `P`) nor to
`ComponentType<P>`, so TS2786.

The port's `jsx_element_type_type_at` asks `instantiated_heritage_base(symbol,
&[])`, which declines a generic alias. Both substitutes were tried and do
not exist in the port: `get_declared_type_of_symbol` of a generic alias is
a print-only `Named` mint with no members (`ElementType<P>`), and
`evaluate_alias_body(symbol, own type parameters)` answers `None`. The
missing piece is the generic alias's declared body type
(`getDeclaredTypeOfTypeAlias`), `declared.rs` (r6-declared2). Once it
exists, the reader here is one line, and the relation of a string literal
to that body must then decide (`relater.rs`).

## 5. TS2609 on an `errorType` spread child — routed, the property-access producer

`inlineJsxFactoryDeclarationsLocalTypes` (`component.tsx` 4:136): the
spread child is `{...this.props.children}` at module level, where `this` is
`undefined` (TS2532, reported). Native's property access on that receiver
answers `errorType`, and `checkJsxExpression` reports TS2609 because
`errorType` is not `anyType`. Traced in the port: `this.props` and
`this.props.children` both answer the intrinsic **`any`** — not a gap, and
not `native_error` — so `check_jsx_expression`'s `ty == any` test (which
is native's own `type != anyType`) declines correctly. Removing the gap
test would change nothing here, and would make TS2609 claims on port gaps
(ADR-0048, `box-protocol.md` §3a). The fix is the producer: property access
on a receiver that `checkNonNullExpression` reduced (TS2532) must answer
`native_error`, not `any` (main's property-access road).

## 6. Hyphenated attributes against an index signature — routed, `relater.rs`

`ignoredJsxAttributes` 20:11. `r5-jsx3.md` §2's decline
(`jsx_hyphen_sensitive_target`) stays: removing it was measured at +1 row
and −5 cases, because the relater relates a hyphenated member like any
other. The prerequisite is `ObjectFlagsJsxAttributes` in the relater, in
the three places native reads it (`relater.go`): `hasExcessProperties`
(`isIgnoredJsxProperty`), `hasCommonProperties` (`isKnownProperty` with
`isComparingJsxAttributes`) and `membersRelatedToIndexInfo` (skip a
hyphenated source member). The source identity is available: every
attributes object is minted by `mint_jsx_attributes_chunk`
(`jsx_intrinsic.rs`), which would record the type in a checker set the
relater reads. Not attempted this session (three relater arms, r6-relater2,
plus a `checker.rs` field); once in, the decline in `jsx_component.rs` is
removed in this lane.

## 7. TS1099/TS1009 on a JSX type-argument list — routed, the parser

6 rows (`jsxIntrinsicElementsTypeArgumentErrors` 5:15, 7:22, 18:15, 20:22;
`tsxTypeArgumentResolution` 26:12, 28:12). `checkGrammarJsxElement`'s
`checkGrammarTypeArguments` reads `NodeList` facts the parser drops in
`parse_type_arguments` (`types.rs`): an empty list is `&[]`, the same as no
list, and the trailing comma returned by `parse_delimited_list` is
discarded. Proposal for the parser (main): in the JSX opening arm
(`jsx.rs`), set `NodeFlags::HAS_TRAILING_COMMA` on the element when the
list had one (a JSX element owns no other comma list, so the flag is
unambiguous), and record a written-but-empty list (no free `NodeFlags` bit
is obvious; a side table keyed by the element, or the `<` position, would
do). The grammar check itself (`check_grammar_jsx_element`) is this lane's:
TS1099 at the list's `pos` (just after `<`) through its end, TS1009 on the
comma. Not attempted here, because the AST change is the larger half.

## 8. Not touched (routed by the brief)

TS7006 in JSX callbacks (`implicit_any.rs`, main), children relations
against tuple/`Iterable` unions (relater/intersections), and
`tsxLibraryManagedAttributes`' alias targets. The managed-attributes alias
also blocks §1's `reactDefaultPropsInferenceSuccess`.

## 9. Diffs, in apply order

All against `2e26f22` plus this lane's commits.

| | File (owner) | What | Measured |
|---|---|---|---|
| J1 | `jsx_intrinsic.rs` (r6-errorsplit2) | `jsx_attributes_inference_type` → `pub(crate)` | required by §1; with §1: +1 case, +8 type lines, 0 lost |
| J2 | `checker.rs` (main) | `jsx_factory_entity`, the option-derived factory text | required by §2; with §2: +1 case, 0 lost |
| J3 | `signatures.rs` (r6-printer4) | an `any` base constructor has no construct signatures | alone: no transition; §3 needs it with the members fix |
| — | `signatures.rs` | alias-reference signatures (`r6-jsx2-alias-reference-signatures.diff`) | **held, not for apply**: +1 case but a false TS2345 in `coAndContraVariantInferences6` (§2) |

The lane's commits **do not compile without J1 and J2**: §1 calls the
builder J1 exposes and §2 reads the field J2 adds.
