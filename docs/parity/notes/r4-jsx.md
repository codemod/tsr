# r4-jsx — JSX element-type checks (`tsr-2zk.907`, under `tsr-2zk.8`)

Round-4 cloud lane notes. Continues `docs/parity/notes/jsx.md` (rounds 1–3);
section numbers there are not reused here. Native source is
`vendor/typescript-go` @ `5b1047d`, `internal/checker/jsx.go`.

## 1. The `JSX.ElementType` branch of `checkJsxOpeningLikeElementOrOpeningFragment`

**Forcing constraint.** `jsx.go:140-150`: when `getJsxElementTypeTypeAt`
answers a type, upstream does not run `checkJsxReturnAssignableToAppropriateBound`
at all. It relates the *tag's* type — `getStringLiteralType(tagName.Text())`
for an intrinsic tag, `checkExpression(tagName)` otherwise — to that type
and reports TS2786 on the tag name, chained over TS2789 `Its type '{0}' is not
a valid JSX element type.`. Until this round `check_jsx_component_bound`
declined every element in such a file (jsx.md §6), so intrinsic tags missing
from a literal `ElementType` never reported (`jsxElementTypeLiteral`: two
TS2786 missing).

**What is ported.** `check_jsx_element_type_constraint` and
`jsx_element_type_type_at` (`crates/tsr-checker/src/jsx_component.rs`).
The branch is taken from inside `check_jsx_component_bound` before its
intrinsic-tag decline, because upstream takes it for intrinsic tags too. The
diagnostic is `NewDiagnosticChain(TS2789, TS2786)` at the tag-name span; the
relation's own elaboration beneath the TS2789 head is not modelled (code,
span and both message texts are upstream's). Only a definite
`Ternary::NotRelated` reports.

**`instantiateAliasOrInterfaceWithDefaults` (`jsx.go:1032`).** With no
written arguments every type parameter takes its default. Reused
`Checker::instantiated_heritage_base(symbol, &[], location)`, which already
implements `fillMissingTypeArguments` over defaults (unknown/any fallback,
JS implicit-any) and `create_type_reference`, which serves both an interface
and a type alias (`getTypeAliasInstantiation`). Rejected: a JSX-local
default filler — a second copy of `fillMissingTypeArguments`. Where the
heritage helper declines (a required parameter with no default in a TS file),
the check is skipped; upstream would instantiate with `errorType`
arguments. No such shape exists in the corpus: the three cases that declare
`JSX.ElementType` (`jsxElementType`, `jsxElementTypeLiteral`,
`jsxElementTypeLiteralWithGeneric`) write none or `P = any`.

**`getJsxElementTypeSymbol`** is `getSymbol(exports, "ElementType",
SymbolFlagsType)`: the export is resolved through an alias and must carry a
type meaning; `jsx_type_symbol` does not filter by meaning, so the filter is
applied here.

**Measured.** `jsxElementTypeLiteral` WRONG → RIGHT; no other verdict moved
in either dump. No cache or side table is added: the constraint is
recomputed per element through `get_declared_type_of_symbol` and the
instantiation table that `create_type_reference` already owns.

**Remaining, not this lane's.** `jsxElementTypeLiteralWithGeneric` line 21:
`ElementType<any>` (a mapped type indexed by `keyof JSX.IntrinsicElements`
with a conditional over `P`, unioned with `React.ComponentType<P>`) is
instantiated, but `"ruhroh"` relates to it as `Related`. Upstream's
`any extends X ? K : never` yields `K`, so the type is
`keyof IntrinsicElements | ComponentType<any>`, which `"ruhroh"` does not
satisfy. The gap is in the mapped/indexed alias instantiation producer
(`declared.rs`, type-aliases lane `tsr-2zk.16.2` on `main`), not here; the
check declines on `Related`, so it costs a missing TS2786, never a false one.
`jsxElementType` line 91 waits on the same plus TS2769/TS2741 elaboration.

**How we would know this is wrong.** A false TS2786 "Its type …" in a file
with `JSX.ElementType` would mean either the default instantiation differs
from upstream's or the tag type does; compare `getJsxElementTypeTypeAt` and
`checkExpression(tagName)` first.

## 2. TS2604: `resolveJsxOpeningLikeElement`'s no-signature arm

**Forcing constraint.** `jsx.go:566-581`: for a value tag whose apparent
type is not `errorType`, `getUninstantiatedJsxSignaturesOfType` (`jsx.go:898`)
— construct signatures of the apparent type, else call signatures, else for a
union `getUnionSignatures` over each constituent's list — is empty and the tag
is not an untyped call (`isUntypedFunctionCall`, `checker.go:9933`, with
construct count 0): TS2604 `JSX element type '{0}' does not have any construct
or call signatures.` on the tag name. Four cases missed exactly that
(`jsxViaImport`, `tsxDynamicTagName7` — a `this` tag —, `tsxElementResolution8`,
`tsxUnionTypeComponent2`).

**What is ported.** `check_jsx_signatureless_tag` and
`jsx_uninstantiated_signatures_are_empty` (`jsx_component.rs`), entered from
`check_jsx_component_bound` before the `ElementType` branch, as upstream
resolves the signature before choosing a bound. The `string` and
string-literal arms of `getUninstantiatedJsxSignaturesOfType` are not this
function's (`anySignature`; `check_jsx_string_literal_tag`). For a union the
port answers "empty" only when some constituent's list is empty, which is
`getUnionSignatures`' first `return nil`; when every constituent has
signatures, whether they combine is the matching half of `getUnionSignatures`
and the port declines. A string or string-literal constituent declines (its
arm can report TS2339 itself). Any unresolved list (`None`) declines.
`isUntypedFunctionCall`'s signature-less arm is re-stated locally rather than
called: `Checker::is_untyped_signatureless_call` is private to `calls.rs`
(calls lane); making it `pub(crate)` and calling it is the integrator's
one-line follow-up.

**Why round 1's version was reverted, and what changed.** Round 1
(`1da6e97`, reverted `6eb0a11`; `jsx.md` §8) reported a false TS2604 on
`reactSFCAndFunctionResolvable`. Root cause, reproduced in a five-line file:
a *qualified* reference to a type alias with no written arguments (`R.SFD`,
`React.SFC`) is minted in `declared.rs`'s QUALIFIED-TYPEREF arm as
`new_named(OBJECT, "R.SFD", Some(alias_symbol))` — a named type whose member
table is the **alias** symbol. `signature_candidates_of_interface_symbol`
reads call/construct members only from interface and type-literal
declarations, so it answers a *complete, empty* list for it. The same file
reports a false TS2349 on `E()` for `declare const E: R.SFD` — the gap is not
JSX's. The unqualified alias (`type S = R.StatelessComponent<{}>`) resolves.

**Accepted decline.** `jsx_uninstantiated_signatures_are_empty` answers
`None` for a named type whose member symbol is a `TYPE_ALIAS`. Upstream has no
such type (an alias's declared type is its body), so its empty list is not
evidence; this mirrors no upstream rule and exists only until the producer
is fixed. Two faithful fixes, neither in this lane's files:

1. `signatures.rs` `signature_candidates_of_named_type`: answer `None` for an
   alias-symbol member table (delivered as `r4-jsx-alias-mint-signatures.diff`,
   measured in the round-4 report), after which this decline is dead code;
2. `declared.rs`: route the argument-less qualified alias reference through
   `getTypeReferenceType` (blocked there on NB-SYMBOL-CHAIN printing).

**Measured.** WRONG → RIGHT: the four cases above. No other case's output
changed in either dump.

**How we would know this is wrong.** A false TS2604 means an empty list this
port treats as complete is not upstream's; compare `signatures_of_type_kind`
for the tag's apparent type against `getSignaturesOfType` before touching
this rule.

**The producer patch, measured** (`r4-jsx-alias-mint-signatures.diff`,
applied on top of this commit, full dumps against the frozen baseline): both
loss checks empty; diagnostics verdicts unchanged; five false diagnostics
removed from WRONG cases — TS2349 ×2 in `jsdocCallbackAndType`, TS2349 ×2 in
`typeTagNoErasure`, TS2345 ×1 in `coAndContraVariantInferences6`. It is
`signatures.rs` (not owned), so it is routed through the report.

## 3. TS2607 from `getJsxPropsTypeFromClassType`

**Forcing constraint.** `jsx.go:953-958`: for a component reference
(`getJsxReferenceKind`, `jsx.go:1159`: construct signatures on the tag's
apparent type) whose `getJsxElementPropertiesName` is a member name, a
signature whose instance type (return type, not `any`) has no such property
reports TS2607 `JSX element class does not support attributes because it does
not have a '{0}' property.` on the element, if the element has any attribute
or spread. Upstream reaches it from `getEffectiveFirstArgumentForJsxSignature`
for each candidate `resolveCall` evaluates.

**What is ported.** `check_jsx_class_attributes_member` (`jsx_component.rs`)
and `jsx_element_properties_member_name` (a new function in
`jsx_intrinsic.rs` over the existing `jsx_container_property`; `None` folds
upstream's `""`, `Missing` and an unenumerable container, so only a definite
member name acts). Only a single non-generic construct signature is answered —
the one candidate certainly evaluated, once; overloads, generics and composite
union signatures (`getJsxPropsTypeForSignatureFromMember`'s per-constituent
arm) decline, as does a union instance. Absence is a complete
`get_property_names_of_type` table without the name. No cache is added.

**Measured.** Three TS2607 lines become right in `tsxElementResolution12`
(23:1, 25:1, 26:1). The case stays WRONG on 33:7 TS2322 (the attribute
relation against the `pr` member's type), which is the attribute-assignability
road, not this rule. No other output changed.

**Remaining.** `tsxSpreadAttributesResolution17`: the instance type of a
class whose base is `any` (`extends React.Component` with `React: any`).
Upstream inherits no properties from an `any` base and adds a
`string → any` index (`resolveObjectTypeMembers`), so `props` is absent and
TS2607 fires; TSR's `get_property_names_of_type` answers `None` for that
base (unfollowable), so the check declines. The fix is in the members
producer (`members.rs`), not this lane.

## 4. The attributes relation: `checkTypeRelatedToAndOptionallyElaborate` for JSX

**Forcing constraint.** Before this round TSR related no JSX attributes type
to its props type at all: of the TS2322/2741/2739/2559 lines baselines record
in `.tsx` files, 5 were produced and 159 (64 cases) were not. Upstream
relates them in two places: the intrinsic arm of
`resolveJsxOpeningLikeElement` (`jsx.go:549-551`, target = the
`IntrinsicElements` member, contextual type = that `& IntrinsicAttributes`)
and `checkApplicableSignatureForJsxCallLikeElement` (`jsx.go:682-698`,
target = `getEffectiveFirstArgumentForJsxSignature`) — both through
`checkTypeRelatedToAndOptionallyElaborate(attributes, target, tagName,
attributes)`.

**What is ported** (`jsx_component.rs`):

- `check_jsx_attributes_assignable` — the call site. The target is what
  `jsx_attributes_context` already resolves and publishes (the intrinsic
  member; for a value tag the single published signature's props, managed
  and intersected as upstream). The source is the attributes builder the
  inference road already uses (`jsx_checked_attributes_type`, a new
  `jsx_intrinsic.rs` wrapper over `jsx_attributes_inference_type(_, false)`).
- `elaborate_jsx_components` — `elaborateJsxComponents`' attributes half
  (`jsx.go:295-305` with `elaborateElement`, `relater.go:546`): target member
  (property, else applicable index), skip an indexed access, relate the
  member pair, elaborate the initializer (through a `JsxExpression`) and
  report on the attribute name via the shared `report_assignability_failure`
  / `check_excess_properties`.
- `jsx_excess_attribute` / `jsx_is_known_property` /
  `report_jsx_excess_attribute` — `hasExcessProperties`' JSX arm
  (`relater.go:2714-2745`): written attributes (and the synthesized
  `children`, reported on the tag) in order, hyphenated names ignored,
  `isEmptyObjectType` not exempting a JSX source, `isKnownProperty` with
  `isComparingJsxAttributes`; TS2322 on the attribute name chained over
  `Property '{0}' does not exist on type '{1}'` (or the `for`→`htmlFor` /
  `class`→`className` / spelling `Did you mean` form).
- Otherwise the shared `report_relation_failure` on the tag name with no
  elaboration node (TS2741 / TS2559 / TS2322 as it decides).

The order is `checkTypeRelatedToAndOptionallyElaborate`'s: `elaborateError`
on the `JsxAttributes` node first (its only arm for that kind is
`elaborateJsxComponents`; `elaborateDidYouMeanToCallOrConstruct` needs call or
construct signatures on an attributes type, which it never has), then the
relation report, in which `hasExcessProperties` precedes every other check
for a fresh attributes source.

**Deviation from the brief, and why.** The brief placed the elaboration arm
in `assignreport.rs` as a patch. Doing that alone would not have been
enough: the excess-attribute report is `hasExcessProperties`' JSX branch,
which the shared reporter does not have (its `is_known_property` is
"without JSX attributes"), and it must run *after* elaboration and *before*
the whole-type report. Both JSX branches are therefore here, applied in
upstream's order at the one JSX call site, with the shared pieces
(`report_assignability_failure`, `check_excess_properties`,
`report_relation_failure`) reused unchanged. If the integrator prefers the
dispatch in `elaborate_error`, the arm is one line —
`Some(Node::JsxAttributes(_)) => return self.elaborate_jsx_components(node,
source, target).unwrap_or(false)` — and moves no measured line, since no
other caller passes a `JsxAttributes` node.

**Declines, each with its upstream reason.**

- A hyphenated attribute name anywhere in the source (written or spread):
  upstream's relater carries `ObjectFlagsJsxAttributes` and treats such a
  name as known/common (`isIgnoredJsxProperty`, `hasCommonProperties`); this
  port's relater has no such flag, and without the decline TS2559 fires
  falsely (`tsxUnionMemberChecksFilterDataProps`, `tsxUnionElementType5`:
  both EMPTY_RIGHT → EMPTY_WRONG in the first draft). Removable when the
  relater learns the flag.
- An `any` (or error) spread: the attributes type is `any` upstream.
- A function tag with a minimum argument count above one:
  `checkTagNameDoesNotExpectTooManyArguments` may answer TS6229 instead.
- Overloaded / unpublished value-tag signatures (`chooseOverload`, calls
  lane), a union target's `getBestMatchingType` /
  `findMatchingDiscriminantType`, an undecided member relation or member
  table: nothing is reported rather than a different line.
- The children half of `elaborateJsxComponents` (TS2745/2746/2747) is not
  ported yet.

**Measured.** WRONG → RIGHT: `tsxAttributeResolution11`,
`tsxElementResolution10`, `tsxElementResolution11`. Right lines added in
still-WRONG cases: `spellingSuggestionJSXAttribute` 2,
`tsxAttributeResolution1` 3, `tsxStatelessFunctionComponents1` 2,
`tsxStatelessFunctionComponents2` 1, `tsxUnionElementType6` 1,
`checkJsxChildrenProperty15` 1, `tsxElementResolution15` 1. One wrong line:
`tsxIntrinsicAttributeErrors` 29:2 reports TS2322 where upstream reports
TS2741 `Property 'key' is missing … in type 'IntrinsicAttributes'` — the
shared reporter's missing-property path does not descend into an
intersection target's failing constituent (`report_relation_failure`,
`assignreport.rs`; not this lane's). Loss checks empty.

**Remaining in this cluster** (most of the 159): elements whose props
`jsx_attributes_context` does not resolve (overloads, generic React
components whose inference declines, destructured-parameter components),
attribute builders that decline, and the children half.

## 5. Remaining clusters, with the blocker each was traced to

Measured at `de35cd6` against the box's frozen baseline (`b23dd3d`).

- **Attribute relation answers `Unknown` against React props (most of the
  159 TS2322/2741/2739/2559 lines; every children case).** Traced on
  `checkJsxChildrenProperty2`: `{ a: number; b: string; }` against
  `IntrinsicAttributes & Prop` is `Unknown`. The relater's reasons probe
  (`relater::reasons`) names `NoMembersTable`: the attributes type is minted
  by `jsx_attributes_inference_type` (`jsx_intrinsic.rs`, the inference
  lane's builder) as a `Named` with `members: None`, so the structural arm is
  never reached. Publishing it as a fresh literal (certifying the member
  table) was tried and moved nothing. Needed: mint it over the binder's
  `JsxAttributes` symbol (`binder.rs:4730` binds it as `OBJECT_LITERAL`), or
  let the relater read `anonymous_properties` for it — a builder/relater
  change outside this lane. Blocks TS2745/2746/2747 (`elaborateJsxComponents`'
  children half: `jsxChildrenIndividualErrorElaborations`,
  `checkJsxChildrenProperty2`/`7`/`14`) and the TS2741 lines in those cases.
- **Weak-type check inside an intersection target.** The same source answers
  `NotRelated` against the `IntrinsicAttributes` constituent alone (no common
  property with `{ key?: Key }`); upstream skips `isPerformingCommonPropertyChecks`
  under `IntersectionStateTarget`. Relater (`relater.rs`), not this lane.
- **TS2741 on an intersection target's constituent** (`tsxIntrinsicAttributeErrors`
  29:2 prints TS2322): `report_relation_failure`'s missing-property path does
  not descend into the failing constituent. `assignreport.rs`.
- **TS2786 for overloaded/union tags** (`tsxElementResolution9`, 2 lines):
  `chooseOverload`'s candidate, calls lane.
- **`ElementType<any>` instantiation** (`jsxElementTypeLiteralWithGeneric`,
  `jsxElementType` 91): §1.
- **TS2607 through an `any` base** (`tsxSpreadAttributesResolution17`): §3.
- **TS2604 after the alias mint is fixed**: §2's decline becomes dead code once
  `r4-jsx-alias-mint-signatures.diff` lands.
