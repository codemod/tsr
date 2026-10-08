# Lane notes: r5-declared (`declared.rs` follow-ons)

Round-5 cloud lane on epic `tsr-2zk`. It takes the `declared.rs` follow-ons
r5-typeparams2 finished its lane before starting: `tsr-2zk.1034`
(conditional-type producers, from [`r5-relater4.md`](r5-relater4.md) §2/§3),
`tsr-2zk.1010` (alias references that enumerate as empty,
[`r5-jsx3.md`](r5-jsx3.md) §6), alias naming of single-constituent
intersections ([`r5-intersections.md`](r5-intersections.md) §3) and
`tsr-2zk.979` (qualified alias and enum references,
[`r5-triage2322.md`](r5-triage2322.md) X10). Owns
`crates/tsr-checker/src/declared.rs`, its own tests and this file. Native
anchors are `vendor/typescript-go` @ `5b1047d` (`internal/checker/checker.go`
unless noted).

Frozen base: `77db6b0` (integration head). `diagverdictdump`: RIGHT 5348,
EMPTY_RIGHT 5581, WRONG 1243, EMPTY_WRONG 66. `verdictdump`: RIGHT 543945,
WRONG 7572, GAP 1016.

**Setup note.** The offline bootstrap's `tomlkit` is unreachable (PyPI 403),
as r5-operators3 §4 recorded. A stdlib-only stand-in for `parse`,
`inline_table` and `dumps`, kept in the session scratchpad and not committed,
assembled the vendored tree. The base `diagverdictdump` was OOM-killed once
when a build and a filtered dump ran beside it (the full dump peaks above
8 GB within two minutes on a 15 GB box); every scoring run below ran alone.

## 1. `tsr-2zk.1034` — conditional-type producers

Committed: (a) the CONDITIONAL flags, (c) the inline distributive constraint
and (d) `forConstraint` in both captures. (b) and (e) are not done; §1.4 says
why. Measured on the final code against the frozen base, unfiltered:

| | before | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 10929 | 10932 (+3 cases) |
| type lines RIGHT | 543945 | 543954 (+9) |
| losses (diag / types) | | 0 / 0 |

Diagnostics gains: `distributiveConditionalTypeConstraints`,
`inlineConditionalHasSimilarAssignability`, `propTypeValidatorInference`
(r5-relater4 §2a's three). Type gains: `distributiveConditionalTypeConstraints`
×7, `conditionalTypes1` ×2. Ir (callgrind, `--singleThreaded --pretty false`):
generic-imports 373,064,328 → 373,066,812 (+0.0007%); domain-model
1,233,634,197 → 1,233,607,085 (−0.002%). Median child CPU vs the base binary:
generic-imports 1.009 (21 samples); domain-model 1.038 at 21 samples, **0.9997**
at 41. `diagnostics_match: true` on both.

### 1.1 (a) Inline deferred conditional mints carry CONDITIONAL

Native's deferred conditional (`getConditionalType`, `:24300`) is a
`TypeFlagsConditional` type wherever it is created. The §906 print-only mint
gave it OBJECT flags outside a mapped template. That is r5-relater4's
[`r5-relater4-inline-conditional-flags.diff`](r5-relater4-inline-conditional-flags.diff),
applied as written. Alone it costs `distributiveConditionalTypeConstraints:0:71`:
`typeof y == 'string'` narrowing of `y: T extends B ? string : number`
(`T extends A`) printed `T extends B ? string : number & string` where native
prints `never`.

**The loss was not in `flow.rs`.** `narrowTypeByTypeFacts` (`flow.go:685`)
keeps a constituent whose facts allow the `typeof`, and `getTypeFacts` of an
instantiable type reads its base constraint. `flow.rs`'s `get_type_facts`
already does that for INSTANTIABLE flags. With OBJECT flags the mint answered
object facts, which deny `typeof == 'string'`: `never`, right by coincidence.
With CONDITIONAL flags it answered the base constraint, and the inline mint had
none, so every fact was allowed. Native's constraint here is the distributive
one, `number` (§1.2), whose facts deny `string`. So (a) is landable only
together with (c), and then it is lossless. No `flow.rs` diff is needed.

### 1.2 (c) The inline distributive constraint

`capture_inline_conditional_constraint` ports
`getConstraintOfDistributiveConditionalType` (`:17286`) for the inline mint and
publishes it into `conditional_constraint_branches`, which
`base_constraint_of_type` (`constraints.rs`) already reads for alias
references. A distributive root (naked type-parameter check) whose check type
has a constraint is evaluated with the check bound to that constraint, one
union constituent at a time, as getConditionalTypeInstantiation distributes
before getConditionalType runs. A `never` result is native's
`noConstraintType`.

- **Ownership/identity.** The key is the mint's `TypeId`. Mints are fresh per
  type node evaluation, private to one `Checker`, and the value is computed
  under the alias frames active when the mint is made, the same frames the
  mint's `conditional_inference_nodes` entry records. It is no new table: the
  existing alias capture already writes the same map.
- **Publication.** Written once, at mint time, as a completed answer. An
  undecided evaluation or relation publishes nothing, which leaves the mint as
  it was before (no constraint).
- **Work boundary.** One `evaluate_conditional_node` per constraint
  constituent, under the existing instantiation-depth guard (declined at 99).
  Inline deferred conditionals are rare on the bench projects (Ir above).
- **Stated divergence: the constraint read.** Native maps the check to
  getConstraintOfType, which for `T extends U` is `U`, and instantiates a
  conditional that is still deferred. This port's evaluator cannot instantiate
  a deferred root under a mapper, so it reads the base constraint, as
  `capture_conditional_alias_branches` already did.

### 1.3 Refused for now: the inline default constraint

getConstraintOfConditionalType falls back to
getDefaultConstraintOfConditionalType (`:17262`), the union of the branches.
Publishing it for inline mints was tried. Filtered on the 11 conditional
cases, against item 1's own binaries:

- with the default branches alone:
  `conditionalTypeAssignabilityWhenDeferred:0:99` RIGHT→WRONG. In
  `const x1: [T] extends [number] ? … = x`, `x` printed `never`.
  getNarrowableTypeForReference substituted `x`'s new union constraint,
  because `context_type_is_generic` (`constraints.rs`) does not count a
  CONDITIONAL type as generic, which native's getGenericObjectFlags does.
- adding that `constraints.rs` arm
  ([`r5-declared-inline-default-constraint.diff`](r5-declared-inline-default-constraint.diff)):
  the same type line still lost, and `distributiveConditionalTypeConstraints`
  went RIGHT→WRONG on diagnostics.

So the diff is −1 type line and −1 case. It is kept as a record, not as a
landable patch. What would change the answer: the narrowing substitution
reading native's `isGenericType` in full (`constraints.rs`, and the
reference's flow type in `flow.rs`), then a re-measure.

### 1.4 Not done: (b) and (e)

- **(b) getConditionalFlowTypeOfType** (`:24952`). Native types a reference
  to the check type inside the true branch as a *substitution type*
  (`getSubstitutionType(T, extends)`). It prints as `T` and relates as
  `T & extends`. This port has no substitution type kind. Adding one changes
  printing and relation everywhere a conditional's true branch mentions its
  check type, so it is an ADR-level change and not a `declared.rs` producer.
  Rewriting `yes` to `T & extends` only inside the constraint capture would
  answer a different question than native's constraint (the default
  constraint, not the branch type), so it was not built either.
- **(e) operands when only the extends type is generic.**
  `conditional_inference_operands` reads the extends operand only for a
  generic check. Its own comment says why: for a concrete check, reading the
  extends type can resolve a recursive `infer` target a second time. Gating
  on roots without `infer` would be a new decline needing its own measurement.
  It is not dispatched in this session; the relater's arms still decline
  (`Unknown`) for such roots, as before.

### 1.5 (d) `forConstraint` in `capture_conditional_alias_branches`

getConditionalType's `forConstraint` arm (`:24383`): when the check type
(the parameter's constraint) is definitely not assignable to the extends type,
the true branch is still added if some constituent of the extends type is
assignable to the check. `for_constraint_includes_true_branch` decides it with
the relater. The permissive instantiations are the types themselves, because a
generic check or extends type has already deferred. Undecided (`Unknown`)
relations publish no constraint. `with_for_constraint_extras` applies it per
distributed constituent to the alias capture's existing evaluator result.
Inline mints use the same helper (§1.2).

- **Stated divergence: `infer` roots.** Native tests the *inferred* extends
  type, which only the evaluator's inference road builds. No extra is added for
  an `infer` root, which is what the port did before. The first draft declined
  the whole capture for `infer` roots instead. That lost three
  `genericConditionalConstrainedToUnknownNotAssignableToConcreteObject` lines,
  because `ReturnType<T[M]>` lost its `unknown` constraint.
- **Stated divergence: chained false branches.** Native's loop applies
  `forConstraint` at each level of a `A ? X : B ? Y : Z` chain. The helper
  tests the outer root only.
- r5-relater4's `for_constraint_extra` (`relater.rs`) adds the same branch on
  the relater's side. With the capture doing it, the relater's union is
  idempotent. Retiring it is r5-relater5's call.

## 2. `tsr-2zk.1010` and the single-constituent alias — held diff

[`r5-declared-intersection-alias.diff`](r5-declared-intersection-alias.diff)
(`declared.rs` plus a unit test) ports getTypeAliasInstantiation over an
intersection body. Items 2 and 3 of the brief turned out to be one change:
native's `instantiateTypeWithAlias(declared, mapper)` re-runs
getIntersectionTypeEx (`:26056`) with the alias, and that function both names
a still-intersection result and returns a reduced singleton before any alias
is attached (`:26127`).

- **Declared type.** getTypeFromIntersectionTypeNode (`:24218`) for a generic
  alias's own body (`Some(_) => error` before) now builds the intersection
  with the alias's own parameters as its arguments (`Name<T>`). It answers the
  reduced type unaliased when the intersection reduces, which is how
  `type Id<T> = { [K in keyof T]: T[K] } & {}` records
  `>Id : { [K in keyof T]: T[K]; }`. The guard is getAliasForTypeNode's: only
  the alias's *direct* body node gets the alias. A nested `keyof (A & B)` does
  not.
- **References.** `create_type_reference_with_display` instantiates the
  declared constituents through the mapper and re-intersects them, so
  `Id<{ a: string }>` is `{ a: string; }`. The namespace-rooted qualified
  generic arm (`React.DetailedHTMLProps<…>` inside `react16.d.ts`) does the
  same and keeps the mint's exact qualified spelling, so no printed line can
  move from that arm. A union result (`keyof T & string` over a concrete `T`)
  carries the alias too: getIntersectionTypeEx hands it to getUnionTypeEx
  (`:26213`).
- **Stated divergence: self-referential bodies** (`type LinkedList<T> = T & {
  next: LinkedList<T> }`) keep the old name mint. With the new arm, `next`
  read `Entity & { next: LinkedList<Entity>; }` where native reads
  `LinkedList<Entity>`. The member read goes through the receiver's
  frame-bound `evaluate_alias_body` (`members.rs`), whose nested reference
  answers the alias-free body. That cost 12 RIGHT lines in
  `recursiveIntersectionTypes`. A first draft also forced the declared type
  while it was resolving and reported a circularity native never sees
  (`any` for `LinkedList`, `Tree`, `DeepPromised`, 47 lines). The arm now
  sits after §29's `deferred_since` park, as the other generic arms do.

**Measured.** The first full run against the frozen base moved 35 cases. Every
later draft was measured filtered on exactly those cases, and the final diff
reads, against item 1's binaries:

- types: +60 lines (with item 1's +9 inside the same cases);
  `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1`–`3`'s
  declared `Id` lines, `parenthesisDoesNotBlockAliasSymbolCreation`,
  `spyComparisonChecking`, `typeVariableConstraintIntersections`,
  `templateLiteralTypes6` and others.
- diagnostics: +3 cases (`jsxChildWrongType`,
  `parenthesisDoesNotBlockAliasSymbolCreation`, `spyComparisonChecking`);
  spellingSuggestionJSXAttribute gains 4 of its missing TS2322 lines and
  tsxLibraryManagedAttributes gains 2. Neither case converts: the rest are
  TS2769 (calls lane) and the `Defaultize` mapped constituents.
- **losses: 1 type line, 1 diagnostics case.** So the diff is held.

The two losses, root-caused:

1. `keyofIntersection:0:17`, `type Result4 = keyof Example4<'x', 'y'>` with
   `type Example4<T, U> = (Record<T, any> & Record<U, any>)`. The operand
   resolves to `any` when the reference is first instantiated inside another
   alias's declared-type computation. Alone in a file the same reference
   prints `Example4<"x", "y">`. `resolved_keyof_type` never sees the
   intersection. Not traced further.
2. `aliasInstantiationExpressionGenericIntersectionNoCrash2` (TS2352 on
   `wat as Wat<string>`). Before, `Wat<number>` was a Named mint, and the
   relater's alias-variance arm compared `Wat`'s arguments. Native probes alias
   variance only for object and conditional types (relater.go
   structuredTypeRelatedToWorker), so an intersection relates structurally:
   `typeof Class<number> & typeof fn<number>` against the `string` form. This
   port's constituents are `ClassAlias<…>`/`FnAlias<…>` name mints (a
   `typeof` query with type arguments is not an alias-free body), so it cannot
   decide that structural relation. The old RIGHT came from a road native does
   not take for this pair. The fix belongs where instantiation expressions get
   real types (`typeof Class<T>`), not in this arm.

What remains for `tsr-2zk.1010` once the diff lands: the JSX cases' last
lines need the `Defaultize` mapped constituents (key-filtered mapped types,
`mapped.rs`, r5-mapped3) and chooseOverload (TS2769). The other
`declarationEmitOptionalMappedTypePropertyNoStrictNullChecks*` lines are
cross-file annotation reuse: native reuses `Id<…>` only where `Id` is
accessible (createApi.ts). In index.ts it prints the structure. That is the
printer's reuse rule, not this producer.

## 3. `tsr-2zk.979` — qualified alias and enum references

`qualified_type_reference` minted an OBJECT `Named` image for every
argument-less qualified reference that the declared-type road could not print
identically (§41). `N.T7` for an alias whose declared type carries the alias,
and a namespace-rooted `First.E`, got that mint. The relater walked a member-less
object: NotRelated as a source (dynamicNames' three extra TS2322 on
`t* = t7`), Unknown as a target (`namespaceDisambiguationInUnion`,
`enumAssignmentCompat3`/`6`).

**Port.** getTypeReferenceType's answer is the declared type
(getTypeFromTypeAliasReference, `:23580`; the enum's declared type). Native
prints it as `N.T7` by qualifying the symbol chain. This port prints at
creation and cannot qualify from the declared type (NB-SYMBOL-CHAIN,
`tsr-e2u`); routing the declared type itself measured 36 R→W in §41. So
`qualified_declared_type_twin` builds a twin that shares the declared type's
semantics and carries the mint's exact text:

- a declared union carrying the alias or enum symbol becomes the same
  constituents and owner symbol (`create_union`), named with the qualified
  text;
- a declared type literal becomes the same member owner (`Named { members:
  literal symbol }`), named with the qualified text. This applies only when no
  TypeId-keyed side table (`anonymous_properties`, `signature_types`,
  `object_literal_index_infos`) holds part of its meaning, because a twin would
  not share it.

Anything else keeps the mint. Twins are memoised in the existing
`qualified_reference_types` table under the existing `(text, symbol)` key. No
new table: the twin replaces the mint the same key held, with the same
lifetime, and is created once per key.

- **Stated divergence: a named union inside a union type node** keeps the
  mint. `get_union_type` answers `errorType` for a union holding a named union,
  because getUnionType's origin denormalisation (`checker.go:25705`) is
  unported. `boolean | X.Foo` printed `any`. That is §41's recorded 3 R→W
  (`enumLiteralAssignableToEnumInsideUnion` 28/35/68), reproduced by the first
  draft.

**Measured** (final code, unfiltered, against the frozen base; item 1
included):

| | after item 1 | after item 4 |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 10932 | 10933 (+1: `namespaceDisambiguationInUnion`) |
| type lines RIGHT | 543954 | 543955 (+1: `styledComponentsInstantiaionLimitNotReached:0:82`) |
| losses vs frozen base (diag / types) | 0 / 0 | 0 / 0 |

Line-level, inside cases that stay WRONG:

- `dynamicNames` loses its three extra TS2322 (main.ts 97:19/46/73).
- `enumAssignmentCompat3` goes from no TS2322 to 18. Native reports 12. The six
  extras are same-named enum pairs, which need isEnumTypeRelatedTo (§3.1).

Ir: generic-imports 373,064,328 → 373,065,770 (+0.0004%); domain-model
1,233,634,197 → 1,232,787,537 (−0.07%). Median child CPU vs the base binary,
21 samples: domain-model 1.0015, generic-imports 0.981.

### 3.1 isEnumTypeRelatedTo — held diff (`relater.rs`)

[`r5-declared-enum-related.diff`](r5-declared-enum-related.diff) ports
isEnumTypeRelatedTo (relater.go:282) into `is_simple_type_related_to`'s
enum-literal arm (relater.go:238). There, two members of different enums with
the same name answered `None` (undecidable). The arm now decides: equal values
*and* related enums. That is same name, both regular enums, and every source
member present in the target with an equal value. An unknown value on one side
is numeric, so it mismatches a known string. Native's per-pair memo
(`c.enumRelation`) is not ported. The walk runs only for same-named enum
pairs, and it would need a `Checker` field (`checker.rs`). `relater.rs` belongs
to r5-relater5, so this ships as a diff.
