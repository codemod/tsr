# r5-jsx3 — JSX attribute checks that declined (`tsr-2zk.982`)

Round-5 cloud lane notes, continuing `r4-jsx.md` and `r4-jsx2.md`. The lane
is bucket M2 of `r5-triage2322.md` (slices D.J1–J6, A2.EJSX): 52 lines in at
most 25 cases where native reports TS2322/TS2345/TS2741/TS2559 on a JSX
element and TSR reports nothing. Native source is `vendor/typescript-go` @
`5b1047d`, `internal/checker/jsx.go` unless noted.

## 1. Namespaced tag and attribute names (J1)

**Forcing constraint.** `isJsxIntrinsicTagName` (`checker/utilities.go:1116`)
is "an intrinsic identifier **or any** `JsxNamespacedName`", and
`getIntrinsicAttributesTypeFromJsxOpeningLikeElement` looks the tag up by
`tagName.Text()`, which for `<ns:element>` is `ns:element`. An attribute's
name is `prop.Name().Text()`, again `ns:attr` for a namespaced one.
`check_jsx_component_bound` returned on any namespaced tag, the attributes
builder (`jsx_attributes_inference_type`) declined any namespaced attribute,
and `jsx_excess_attribute` declined on one.

**What is ported** (`jsx_intrinsic.rs`): `jsx_namespaced_text`,
`jsx_attribute_name_text` and `jsx_intrinsic_tag_text` give upstream's
`Text()`; the intrinsic-table lookup, the attributes builder, the contextual
attribute type and the discriminator read through them. In
`jsx_component.rs` a namespaced tag takes the intrinsic road (the attributes
relation and the `JSX.ElementType` string-literal arm), and elaboration and
the excess check read namespaced names.

**Printing.** A JSX attribute member prints as `getPropertyNameNodeForSymbol`
prints a symbol: an identifier as written, anything else (`data-foo`,
`ns:attr`) as a string literal (`jsx_printed_member_name`). Before this, a
hyphenated attribute printed unquoted (`{ data-foo: number; }`); upstream
prints `{ "data-foo": number; }`. The `Did you mean` suggestion is a
`symbolToString`, so it is quoted the same way (`'"ns:attribute"'`).

## 2. Hyphenated attribute names (J2)

**Forcing constraint.** The decline was "any hyphenated attribute, anywhere,
declines the element", because this port's relater has no
`ObjectFlagsJsxAttributes`. Upstream reads that flag in exactly three places
(`relater.go`): `hasExcessProperties` (`isIgnoredJsxProperty`, already
ported in `jsx_excess_attribute`), `hasCommonProperties` (`isKnownProperty`
with `isComparingJsxAttributes`: a hyphenated name is known), and
`membersRelatedToIndexInfo` (a hyphenated source member is skipped against
an index signature). Everywhere else a hyphenated member is an ordinary
property: `<test1 data-foo={32} />` against `{ "data-foo"?: string }` fails
on the member's type (`tsxAttributeResolution7`).

**What is ported.** `jsx_hyphen_sensitive_target` declines only where one of
the two relater rules can change the answer: some object constituent of the
target has index signatures, or is weak (every property optional) and lacks
one of the hyphenated names. A weak constituent that declares every
hyphenated name knows it either way, so the flagless relation is upstream's.
An unreadable table declines. Only the minted attributes object carries the
flag; an intersection over a generic spread (§4) does not.

**Rejected.** Teaching the relater the flag (`relater.rs`, r5-relater3's
file); the narrowed decline is removable when it lands.

## 3. The excess check runs before the relation (attributes freshness)

**Forcing constraint.** `<a:b a="accepted" b="rejected" />` against
`{ a: string }` (`jsxElementType` 111:19): the structural relation is
`Related` (extra members are allowed), so the check stopped, but upstream
fails it in `isRelatedTo` because the attributes type is a fresh object
literal and `hasExcessProperties` runs first. The port ran
`jsx_excess_attribute` only after a `NotRelated`.

**Which attributes types are fresh.** `createJsxAttributesTypeFromAttributesProperty`
(`jsx.go:709-866`) adds `ObjectFlagsFreshLiteral` to its shared `objectFlags`
only inside `createJsxAttributesType`, i.e. when it builds a chunk of
**written** attributes (or the empty type for an element with none). Spread
chunks and the children chunk are `getSpreadType` results stamped with the
flags *as they stand* (Go evaluates `createJsxAttributesType()` before the
`objectFlags` argument, so a written chunk's spread is fresh). So an element
with no written attribute — only spreads, only children — is not fresh and
gets no excess check; children after a written attribute are checked
(`checkJsxChildrenProperty15`: TS2322 `Property 'children' does not exist`).
A generic spread leaves an intersection, which is not an object literal.
`jsx_excess_attribute` returns `JsxExcess::None` for both.

**What is ported.** `check_jsx_attributes_assignable` computes the excess
member first. Without one, the relation decides as before. With one, the
relation fails — but only when every object constituent of the target has
a member table the port can trust (`jsx_target_members_complete`): a
complete declared or object-literal table, or an instantiated class or
interface reference that `get_property_names_of_type` enumerates through
its heritage (`IntrinsicClassAttributes<T>`). A type-alias reference is not
trusted: §6.

**Measured on the way.** The first draft (excess first, no freshness, no
completeness guard) converted 6 cases and lost 25 configured and plain
EMPTY_RIGHT rows: a children-only element against `{}` (freshness), and
`Defaultize<…>` / `DetailedHTMLProps<…>` targets whose alias reference
enumerates as empty (§6). Both guards together: no loss.

**Automatic runtime the port cannot resolve.** With the excess check first,
`jsxNamespaceImplicitImportJSXNamespaceFromPragmaPickedOverGlobalOne` gained
a false TS2322: its `@jsxImportSource @emotion/react` pragma is not read by
the loader, so `getJsxNamespaceAt`'s first road (the runtime module) misses
and the global React namespace answers instead of `EmotionJSX`. When the
file selects the automatic runtime (`jsx_implicit_import_base`) and
`jsx_implicit_import_container` does not resolve, the check now declines:
the namespace found next is not upstream's, and where the module truly is
missing upstream reports TS2875 (not ported). Removable when the loader
reads the pragma.

## 4. Generic spreads (J4)

**Forcing constraint.** `getSpreadType`'s generic arm (`checker.go:13419`):
a spread of a generic object type is not flattened. Against an empty left
it is the spread itself (`<test1 {...obj} />` has attributes type `T`);
otherwise an intersection, merging a trailing non-generic chunk into the
intersection's last non-generic member. The builder called
`spread_properties` on `T`, which declines, so the element declined.

**What is ported** (`jsx_attributes_inference_type`): when
`spread_generic_flags(spread).0` (`isGenericObjectType`), the written chunk
so far is minted (`mint_jsx_attributes_chunk`) and pushed, then the spread;
later chunks and the children chunk merge into the trailing object part.
The result is the single part or `get_intersection_type` of the parts.
`T` relates to the props as any type parameter does; `tsxAttributeResolution5`
(TS2322 `Type 'T' is not assignable to type 'Attribs1'`) and
`tsxGenericAttributesType7`/`8` (`'U'` vs `'IntrinsicAttributes & U'`).

**Rejected.** Calling `spreads.rs`'s `get_spread_type` (it is private to
that file, and the builder's non-generic chunk merge predates it). Making
it `pub(crate)` and folding the builder onto it is the integrator's
follow-up; the generic arm here is that function's, line for line.

## 5. Results

Frozen baseline `a7d108d` (this box's start). Full dumps (plain and
configured keys), both §5 loss checks empty, no new wrong line in any case
(line-level diff of every row).

| | plain diag RIGHT+EMPTY_RIGHT | configured rows changed | type lines RIGHT |
|---|---|---|---|
| baseline | 8,653 / 9,816 | — | 543,125 |
| §1–§4 | **8,662** | +2 (WRONG → RIGHT) | 543,125 |

WRONG → RIGHT: `jsxNamespacePrefixIntrinsics`, `tsxAttributeResolution5`,
`…7`, `…15`, `tsxGenericAttributesType7`, `…8`, `checkJsxChildrenProperty15`,
`tsxSpreadAttributesResolution14`, `tsxUnionElementType4`, and
`jsxNamespaceElementChildrenAttributeIgnoredWhenReactJsx` under
`jsx=react-jsx` and `jsx=react-jsxdev` (its TS2741 was a TS2322 because the
excess `offspring` now fails the relation first, §3). Right lines added in
still-WRONG cases: `jsxElementType` 110:6 and 111:19,
`contextuallyTypedStringLiteralsInJsxAttributes02` 37:57 and 40:44.

**Performance** (median child CPU against the frozen binary; the bench
projects contain no JSX): domain-model 0.981 (21 samples), generic-imports
1.053 at 21 → 0.946 at 41; diagnostics match. No cache, side table or
traversal is added; the excess walk now runs once per checked element
before the relation instead of after a failed one.

## 6. Remaining, with the blocker each was traced to

- **Alias references enumerate as empty objects (J3, J6; ≈29 lines).**
  `React.DetailedHTMLProps<…>` and `Defaultize<…>` are minted as
  `Named { members: Some(alias symbol) }` with a `type_reference_targets`
  entry; `get_property_names_of_type` answers `Some([])` and the relater
  relates any object to them (`Related`), so `<a class="">` and every
  `tsxLibraryManagedAttributes` element are silent. The body is available
  (`binding_type_alias_body` gives `ClassAttributes<HTMLElement> &
  React.HTMLAttributes<HTMLElement>`). The faithful fix is in the producer
  (`members.rs` names / the relater's structural arm reading the alias body,
  or `declared.rs` minting the body with an alias attached) — not this
  lane's files. This is the same producer shape `r4-jsx.md` §2 met for
  signatures.
- **Fragments (J5, `jsxFragmentWrongType` 6:28).** `getJSXFragmentType`
  (`jsx.go:500`) → `resolveCall` on `React.Fragment`'s signatures with a
  children-only attributes type, error node the opening fragment. Not
  ported here.
- **Children elaboration against intersection/union children types**
  (`checkJsxChildrenProperty4`, `jsxChildrenIndividualErrorElaborations`):
  `r4-jsx2.md` §5's declines (union children target needs
  `getBestMatchingType`).
- **Overloaded class components (TS2769)** — `chooseOverload`, calls lane.
- **Message text.** Optional members print without `| undefined` in some
  heads (`tsxAttributeResolution7`: native `"data-foo"?: string | undefined`);
  the printer, not this lane.
