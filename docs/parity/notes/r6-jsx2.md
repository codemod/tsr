# r6-jsx2 — JSX diagnostics, continued (`tsr-2zk.1147`)

Round-6 lane notes, continuing `r6-jsx.md` (§7 is this lane's board) and
`r5-jsx3.md`. Native source is `vendor/typescript-go` @ `5b1047d`,
`internal/checker/jsx.go` unless noted. The lane owns `jsx_attributes.rs`,
`jsx_component.rs` and `jsx_factory.rs`; everything else is shipped as a
measured diff (§9).

## 0. The frozen base

Batch BQ (r6-jsx) had not landed on `claude/beautiful-shannon-ar5gh0` when
this lane started, and every item here builds on r6-jsx's chooser. The base
is therefore what BQ will land: main `5e4d21b` merged with r6-jsx's lane tip
`8d5fa73` (the branch's first commit; the snapshot, `round5.md` and Beads
conflicts were resolved to main's side, since the integrator regenerates
them), with r6-jsx's two held diffs applied on top:
`r6-jsx-tag-name-value-reference.diff` (D1, `check.rs`) and
`r6-jsx-resolver-chooses-overloads.diff` (D2, `jsx_intrinsic.rs`). Every
diff in §9 is against that tree.

| | diagnostics RIGHT | EMPTY_RIGHT | type lines RIGHT |
|---|---|---|---|
| frozen base | 5621 | 5606 | 550,285 |

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
