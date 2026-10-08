# r4-jsx2 — the JSX attributes type and the children elaboration (`tsr-2zk.907`)

Round-4 cloud lane notes, continuing `r4-jsx.md` (whose §4–§5 this lane picks
up). Native source is `vendor/typescript-go` @ `5b1047d`,
`internal/checker/jsx.go` unless noted.

## 1. The attributes type is an anonymous object over the `JsxAttributes` symbol

**Forcing constraint.** `r4-jsx.md` §5: the attributes relation in
`check_jsx_attributes_assignable` answered `Unknown` for every React
component, and `relater::reasons` named `NoMembersTable`. Reproduced on
`checkJsxChildrenProperty2`: the source `{ a: number; b: string; }` was
`Named { members: None }`, so `Relater::has_members` (`relater.rs`) refused it
and the structural arm was never reached.

**Why publishing it as a fresh literal moved nothing (r4-jsx's attempt).**
`has_members` asks only for `TypeData::Named { members: Some(_) }` — freshness,
`anonymous_properties` and `object_literal_members` are read *after* that gate.
A fresh mint with no symbol stays outside the structural arm.

**What upstream builds.** `createJsxAttributesTypeFromAttributesProperty`
(`jsx.go:709`) makes every attributes chunk with
`newAnonymousType(attributesSymbol, attributesTable, …)`, where
`attributesSymbol = attributes.Symbol()` — the binder's `__jsxAttributes`
symbol (`binder.rs`, `anonymous_declaration`, `OBJECT_LITERAL`) — and the
children chunk with the same symbol. The object literal road in this port does
the same (`objects.rs`: `new_named(OBJECT, printed, symbol_of(node))`).

**What is ported.** `jsx_attributes_inference_type` (`jsx_intrinsic.rs`) mints
over `binder.symbol_of(attributes)` instead of `None`. The member table the
relater and member reads use is still the captured `anonymous_properties`
entry (complete, `true`), which `get_type_of_property_of_type` and
`get_property_names_of_type` read before the symbol's own table, so spread and
synthesized `children` members (which have no binder declaration) resolve as
before. No table, cache or traversal is added.

**Not ported, and why.** Upstream also sets `ObjectFlagsFreshLiteral |
ObjectLiteral | JsxAttributes`. Freshness would route the relater's own
`hasExcessProperties` (the non-JSX arm: no `isHyphenatedJsxName`, no `children`
on the tag) over the source; the JSX arm is `jsx_excess_attribute`
(`r4-jsx.md` §4), which runs first at the one call site. Marking it fresh
needs the relater's `ObjectFlagsJsxAttributes` arm first (`r4-jsx.md` §4,
declines).

## 2. The children half of `elaborateJsxComponents`

**What is ported** (`jsx_component.rs`): `elaborate_jsx_children` —
`jsx.go:306-363` — and `elaborate_jsx_children_elementwise` —
`elaborateIterableOrArrayLikeTargetElementwise` (`jsx.go:419`) over
`generateJsxChildren` / `getElaborationElementForJsxChild`.

- The target's children member is `getIndexedAccessType(target, "children")`;
  a target with no such member reports nothing (upstream's `unknown` there
  relates everything and `getBestMatchIndexedAccessTypeOrUndefined` misses).
- Its constituents split by assignability to `Iterable<any, void, undefined>`
  (`createIterableType(anyType)`) when the global `Iterable` exists, else by
  `isArrayOrTupleLikeType` (`binding_parent_is_array_like` or a `"0"` member).
- Several children, an array-like part: element-wise, index counting skipping
  only whitespace text (an empty `{}` keeps its index, as upstream's counter
  does); each target is the non-array-like parts' iterated type
  (`get_iteration_types_of_iterable`, `FOR_OF`) unioned with the array-like
  part's element at that index.
- Several children, no array-like part: a failed `children` relation is TS2746
  on the tag name; one child, no non-array-like part: TS2745.
- One child otherwise: `elaborateElement` — text reports TS2747 through the
  custom diagnostic factory (`'{tag}' components don't accept text …`), an
  expression child reports on the `JsxExpression` with its inner expression
  elaborated, an element child is both.

**Declines.** Any `Unknown` relation, an undecidable array-likeness, a union
target in the single-child arm (`getBestMatchingType`), a union of several
array-like parts in the element-wise arm, an unreadable children container
(`""` or several properties). Each costs a missing line, never a different one.

## 3. Prerequisite outside this lane: the relater's missing-member arm

Minting the attributes type with a member table exposed two relater defects
that had been hidden behind `NoMembersTable`. Both are in `relater.rs` /
`members.rs` (not owned); the hunk is `r4-jsx2-relater-heritage-flags.diff`.

1. **§387 answers `NotRelated` without knowing the target member is
   required.** `properties_related_to_with_optionals`: a target member absent
   from a complete source is `NotRelated` whenever `property_flags` did not
   say *optional* — including when it said nothing (`None`). Reproduced
   without JSX: `interface M<T> { ref?: T } interface N extends M<number> {}
   declare let e: {}; let c: N = e;` reports a false TS2322. Every class
   component's target contains `JSX.IntrinsicClassAttributes<T>`, which is
   exactly that shape (`extends React.ClassAttributes<T> {}`), so every class
   component element became a false TS2322 (16 cases EMPTY_RIGHT/RIGHT →
   WRONG). Fix: require `target_metadata.is_some()` for that `NotRelated`.
2. **`property_flags` cannot see a member inherited through a generic base.**
   `get_property_of_type` deliberately refuses generic heritage
   (`base_symbols_of`, §202: the symbol's type would be uninstantiated), so
   optionality was unknown for `JSX.Element extends React.ReactElement<any> {}`
   and with (1) alone the children relations became `Unknown`. Fix:
   `Checker::generic_heritage_property_symbol` (`members.rs`), the same walk as
   `generic_heritage_member` answering the declaring symbol, read by
   `property_flags` only — its modifiers are what upstream's instantiated
   symbol (`instantiateSymbol`) carries.

Measured: see §5.

## 4. A harness hazard: multi-valued `@jsx`

`jsxNamespaceNoElementChildrenAttributeReactJsx` writes `@jsx:
react-jsx,react-jsxdev`. `apply_test_directives` (`tsr-conformance`) parses a
comma list as no value, so the case runs with `jsx` unset, where
`getJsxElementChildrenPropertyName` has no `ElementChildrenAttribute` to read
and no `children` member is synthesized. With §1 and only hunk (1) of §3
applied, the decided relation reported TS2741 twice there (EMPTY_RIGHT →
EMPTY_WRONG); under the declared variant (`@jsx: react-jsx`, children fixed by
`JsxEmitReactJSX`) the port reports nothing, matching the baseline. With the
full change the case stays EMPTY_RIGHT, but it is judged in a configuration
upstream never ran, so its verdict is not evidence either way. 22 corpus cases
write a comma list for `@jsx`; running each declared variant is the harness's
job (`tsr-bb4.1` covers configuration-varied baselines).

## 5. Measurements

Against the box's frozen baseline (`fdb11b4`, the r4-jsx head merged with the
integration branch), full dumps, both §5 loss checks:

| State | diag RIGHT / WRONG | type lines RIGHT | losses |
|---|---|---|---|
| baseline | 4257 / 1245 | 469,980 | — |
| §3 hunk alone (`r4-jsx2-relater-heritage-flags.diff`) | 4257 / 1245 | 469,980 | 0 / 0 (no verdict or line changed) |
| §3 hunk (1) alone, without (2) | 4257 / 1245 | 469,953 | 0 / 27 (`temporal`: an overload choice that leaned on the false `NotRelated`) |
| §1 alone (before §2, without §3) | 4275 / 1227 (EMPTY_WRONG 98 → 110) | 469,981 | 16 / 0 (class components: §3 (1); one harness case, §4) |
| **§1 + §2 + §3** | **4282 / 1220** | **469,981** | **0 / 0** |

So this lane's code is only loss-free together with the §3 hunk; the hunk
alone changes nothing, and (2) is what keeps (1) from losing `temporal`.

WRONG → RIGHT with everything (25): `jsxChildrenGenericContextualTypes`,
`jsxEmptyExpressionNotCountedAsChild2`,
`jsxIntrinsicDeclaredUsingTemplateLiteralTypeSignatures`,
`tsxDeepAttributeAssignabilityError`,
`tsxTypeArgumentPartialDefinitionStillErrors`, `checkJsxChildrenProperty7`,
`checkJsxChildrenProperty14`, `checkJsxGenericTagHasCorrectInferences`,
`contextuallyTypedStringLiteralsInJsxAttributes01`, `tsxAttributeErrors`,
`tsxAttributeResolution1`, `…3`, `…6`, `…9`, `…10`, `…12`, `…14`,
`tsxDefaultAttributesResolution3`, `tsxElementResolution3`, `…4`, `…12`,
`tsxSpreadAttributesResolution10`, `…12`, `tsxUnionElementType1`, `…2`.
One type line: `jsxHasLiteralType:0:9`.

Still WRONG in the brief's children cases: `checkJsxChildrenProperty2` 16:10
(TS2322 where upstream reports TS2741 — `report_relation_failure` does not
descend into the intersection target's failing constituent, `assignreport.rs`,
`r4-jsx.md` §5) and `jsxChildrenIndividualErrorElaborations` (38:4 TS7006, an
implicit-`any` arrow child under a `Cb[]` children type; 63:3/67:16 and 73:9/74:9,
the `Cb | Cb[]` union children target, where the single-child arm needs
`getBestMatchingType` and the element-wise arm the union of array-likes).

**Performance** (combined state against the frozen binary, median child CPU,
21 samples): domain-model 0.978, generic-imports 1.003; diagnostics match.
Callgrind on `generate_perf_project.py --modules 100`: base 4,222.7M and
4,228.9M on two identical runs, combined 4,235.5M (+0.15–0.30 %); the new
heritage walk is 4.1M inclusive (0.10 %), entered only when
`get_property_of_type` misses in `property_flags`.

**How we would know this is wrong.** A false TS2322/TS2741 on a JSX element
whose props a plain object literal satisfies means the attributes type's
member table differs from upstream's; compare `get_property_names_of_type` on
the source with the element's written attributes, spread members and
synthesized `children`. A false TS2745/2746 means the iterable split differs:
check `relate_ternary(part, Iterable<any, void, undefined>)` per constituent.

**Coverage run** (combined state; "before" is the frozen baseline source):
`checker_types` 8081 → 8082 / 9538, `diagnostics` 4257 → 4282 / 5502.
