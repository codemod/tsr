# r5-mapped4 — mapped printing, node reuse, mapped iteration (`tsr-2zk.1033`)

Round-5 cloud lane, successor to r5-mapped3 (`r5-mapped3.md`). Owns
`crates/tsr-checker/src/mapped.rs`, `intersections.rs` (printing paren rules),
`node_reuse.rs` and `printing::prints_as_a_single_token`. `declared.rs`
(r5-declared) and `relater.rs` (r5-relater6) changes ship as measured diffs.
Native source is `vendor/typescript-go` @ `5b1047d`.

Baseline frozen at `ccb48e7` (`integrate: merge …-r5-vardecl`), before batch
W's enum-keys diff reached the integration branch: types 544,166 RIGHT /
988 GAP / 7,379 WRONG of 552,533 aligned lines; diagnostics 5,368 RIGHT +
5,584 EMPTY_RIGHT of 12,238 rows.

## 1. Intersection constituents parenthesised by node precedence (committed)

`emitTypeNode(node, TypePrecedenceIntersection)` (`printer.go:2274`) wraps a
constituent whose node binds below `Intersection` (`ast/precedence.go`:
`Conditional < JSDoc < Function < Union < Intersection`). `create_intersection`
decided by type kind instead: a union without an alias symbol, or a
signature type. Two kinds came out wrong:

- **A union printing a non-union origin.** getIndexType attaches
  `origin = newIndexType(t)` to a key union; typeToTypeNode prints the origin,
  a `TypeOperator` node, which is never wrapped. The port wrapped it:
  `P & (keyof NameMap)`. The same holds for an alias-spelled distributed
  union (mapTypeWithAlias, a `TypeReference`) and an intersection origin.
- **A conditional constituent** binds below `Intersection` and printed
  unwrapped: `T extends C ? number : string & string`.

The constituent's node kind exists in this port only as its printed text (a
union's text *is* its origin's print, `unions.rs` `create_union_with_text`), so
the precedence is read off the text by `node_reuse::text_precedence`, the
reader the node-reuse fallback already uses for the same question
(`binds_below_intersection`).

**Alternative rejected.** Recording the origin's kind on the union at mint
time is more direct, but the `keyof` origins are minted in `declared.rs`
(`keyof_origin_applies` callers) and `unions.rs`' `union_with_origin_text`
takes only text; both are outside this lane. If the origin ever becomes a
type rather than text (`tsr-2zk.16.99`'s operand work), the kind should be
read from it and the text reader retired here.

**Measured** against the frozen base, both dumps unfiltered: types +3 RIGHT
(`distributiveConditionalTypeConstraints:82/93`, `mappedTypeAsClauses:103`),
diagnostics unchanged, zero losses on both. On top of r5-mapped3's
declared-route diff it clears that diff's 13 `(keyof X)` losses
(`reverseMappedTypeIntersectionConstraint` ×10, `mappedTypeAsClauses:101/102`,
`reverseMappedTupleContext:45`).

**Falsifier.** A printed constituent whose text the reader misclassifies:
a text with a top-level `|`, `&`, `=>` or ` extends … ?` that is not that
node kind. Type texts this port prints put such tokens at depth zero only in
the node kinds they name.

Full parity run (`coverage`) after: checker_types 8,236 of 9,538
(configured 1,643 of 1,928); diagnostics 4,531 of 5,502 (configured 837 of
1,089). Perf, median child CPU new/old: domain-model 0.982 (21 samples),
generic-imports 1.033 at 21 samples, 0.989 at 41; `diagnostics_match: true`.
Callgrind Ir (`--singleThreaded --pretty false`): domain-model 1,201,918,429
→ 1,201,832,316, generic-imports 343,420,545 → 343,407,345 (both −0.01%).

## 2. Type-parameter constraints reuse their written node (committed)

`typeParameterToDeclaration` (`nodebuilderimpl.go:1611`) prints a constraint
through `typeToTypeNodeHelperWithPossibleReusableTypeNode(constraint,
getConstraintDeclaration(parameter))` (`:1597`): when `getTypeFromTypeNode`
of the first written `extends` node (`getConstraintDeclaration`,
`checker.go:29132`) *is* the constraint, the node is re-emitted through
`tryReuseExistingNodeHelper`; otherwise the constraint is serialized. The
port's signature printers only had `TypeParameter::written_constraint`, a
set of syntactic admissions in `signatures.rs`, and otherwise rendered the
type. `node_reuse::reused_constraint_text` is the native rule, asked after
the existing admissions and before the fresh render, at both `signatures.rs`
print sites (with a site, `written_annotation_text_at`; without,
`site_free_annotation_text`).

The identity gate exposed a second gap: **a mapped node minted a new type on
every evaluation**, so `getTypeFromTypeNode(node) == constraint` never held
for `{ [P in string]: TakeString }`. Native's getTypeFromMappedTypeNode
(`checker.go:24170`) caches on `typeNodeLinks.resolvedType`.
`create_semantic_mapped_type` now publishes its answer in
`type_literal_types`, the table type-literal and function-type nodes use for
the same links.

**Measured** against the frozen base, both dumps unfiltered, with §1:
types +38 RIGHT (+35 from this change), diagnostics unchanged, zero losses
on both. Converted (this change): `genericFunctionsAndConditionalInference`
×9, `noUncheckedIndexedAccess` ×6, `correlatedUnions` ×5, `typeAliases` ×3,
`bindingPatternCannotBeOnlyInferenceSource` ×2,
`divideAndConquerIntersections` ×2, `mappedTypeIndexedAccessConstraint` ×2,
`cannotIndexGenericWritingError`, `declFileRestParametersOfFunctionAndFunctionType`,
`inKeywordTypeguard(strict=false)`, `inlinedAliasAssignableToConstraintSameAsAlias`,
`spreadObjectOrFalsy`, `typeParameterConstraints1`. On top of r5-mapped3's
declared-route diff it clears that diff's remaining 6 losses
(`mappedTypeContextualTypesApplied` ×4,
`contextualTypeBasedOnIntersectionWithAnyInTheMix3` ×2).

**Not taken: reuse before the admissions.** Native has no admission list;
the faithful order is reuse first. `written_constraint` is filled in
`signatures.rs` (not this lane's), and for every node it admits, reuse
answers the same text or a written spelling it chose deliberately (a
qualified name kept, §926). Replacing it is `signatures.rs`' call.

**Two more printers are not changed.** `objects::signature_member_text` and
`Checker::signature_member_text_at` (`checker.rs`) print a member
signature's type parameters with the same `written_constraint`-then-render
rule. They need the same one-line arm; both files are outside this lane
(reported to the integrator).

### Ownership and work boundaries (checker port convention)

- **Native operations:** `typeToTypeNodeHelperWithPossibleReusableTypeNode`
  from `typeParameterToDeclaration`; `getConstraintDeclaration`;
  getTypeFromMappedTypeNode's `typeNodeLinks.resolvedType`.
- **Key identity and owner:** the mapped node's type is keyed by
  `TypeLiteralKey` (the node, the active alias-evaluation bindings, whether a
  mapped template encloses it), owned by `type_literal_types` on the
  `Checker`. The bindings and template flag stand for the mapper context in
  which this port re-resolves a node where native instantiates one type.
- **Publication states:** published once a build succeeds (generic: the
  deferred mapped type with its `mapped_types` info; non-generic: the
  resolved object). A declined build publishes nothing, as before, so the
  written-text fallback still runs and a later evaluation retries. No
  reservation: a node whose build re-enters itself recomputes, as before.
- **Receiver/alias context:** the reuse is decided against the type
  parameter's own declaration; an instantiated or renamed parameter's
  constraint is a different type and is serialized.
- **Expensive work boundary:** the cache removes repeated member resolution
  of the same non-generic mapped node (each evaluation previously rebuilt
  it). The reuse walk runs only for a printed constraint the admissions did
  not take, once per print, as `written_annotation_text_at` does for
  parameters.

**Falsifier.** A mapped node whose built type depends on checker state not
in `TypeLiteralKey` (anything but the alias bindings and the template
depth) would now print or relate its first context's image. The
`mapped_type_info` inputs are the node, `get_declared_type_of_symbol` of its
parameter and `get_type_from_type_node` of its parts, which are keyed the
same way.

Full parity run after §2: checker_types 8,236 → 8,242 of 9,538 (configured
1,643); diagnostics 4,531 (configured 837). Perf, median child CPU new/old
(21 samples, vs the frozen base binary): domain-model 0.987, generic-imports
1.028; `diagnostics_match: true`. Callgrind Ir: domain-model 1,201,902,760 →
1,200,935,103 (−0.08%), generic-imports 343,419,388 → 343,421,473 (+0.001%).

## 3. Linear member-table merge in resolveMappedTypeMembers (committed)

Re-measuring r5-mapped3's declared-route diff (§4) showed
`hugeDeclarationOutputGetsTruncatedWithError` going from 172 ms to 8,440 ms
(45 → 375 MiB). Its `{ [K in manyprops]: { [K2 in manyprops]: … } }` has
676 × 676 members. Callgrind put 34% of Ir in `String::clone` and 14% in
`memcmp` under `resolve_mapped_type_members_worker`: two scans were
quadratic in the key count. The key-merge `find` rebuilt
`mapped_key_property_name` (a new `String`) for every earlier member on
every key, and the duplicate-name check walked every earlier property.

Native keys its member table by name (`resolveMappedTypeMembers` →
`addMemberForKeyTypeWorker`, a symbol table lookup). The port now keeps a
name → first-member index and a name set. The first member a name lands on is
the one the old `find` returned, because a later key with the same name
always merged into that member and never became a member itself. So the
result is identical.

**Measured:** both dumps byte-identical to the base (key, verdict, want, got)
and no slow cases. On that file's CLI check with the route applied: 7.39 s →
2.46 s. Bench Ir: domain-model 1,195,807,596 → 1,195,675,981 (−0.01%),
generic-imports 342,893,926 → 342,907,113 (+0.004%). Median CPU new/old:
domain-model 1.029, generic-imports 0.962 (21 samples).

## 4. r5-mapped3's declared-route diff, refreshed (held: one slow case)

[`r5-mapped4-declared-route.diff`](r5-mapped4-declared-route.diff) is
r5-mapped3's [`r5-mapped3-declared-route.diff`](r5-mapped3-declared-route.diff)
rebased onto the current `declared.rs`, unchanged in substance. A mapped node
goes through `create_semantic_mapped_type` (createMappedTypeNodeFromType from
typed parts; members when not generic) before the written-text mint, and
`keyof any|never|unknown` goes to `resolved_keyof_type` (getIndexTypeEx's
arms, checker.go:26701, `.16.108`).

**Measured** unfiltered on top of §1 and §2 (base `22f35b4`, which already
carries them): types **+81 RIGHT**, diagnostics **+1 case** (`bigintIndex`),
**zero verdict losses** on both dumps, including base-RIGHT keys missing from
the new dump (gate v3). Its 19 losses at r5-mapped3's measurement are §1's 13
and §2's 6. Converted lines by case: `verbatim-declarations-parameters` ×7,
`paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized` ×7,
`assignmentGenericLookupTypeNarrowing` ×6, `deeplyNestedMappedTypes` ×5,
`bigintIndex` ×5, `mappedTypeWithAny` ×4,
`declarationEmitMappedTypePropertyFromNumericStringKey` ×4,
`mappedTypeModifiers` ×3, `mappedTypeAsClauses` ×3,
`dependentDestructuredVariablesFromNestedPatterns` ×3,
`typeGuardNarrowsIndexedAccessOfKnownProperty11`/`12` ×3 each, and 17 more
cases at 1–2 lines. Perf: median CPU new/old domain-model 0.983,
generic-imports 1.006; Ir domain-model 1,195,825,471 → 1,202,251,201
(+0.54%), generic-imports 342,899,754 → 342,903,933.

**Why it is held.** `slowcases` flags one case. Before §3,
`hugeDeclarationOutputGetsTruncatedWithError` went from 172 ms / 45 MiB to
8,440 ms / 375 MiB; after §3 it is **2,060 ms / 376 MiB**. That is still
above 3× the base and above the 1 s and 256 MiB floors. The case's
`{ [K in manyprops]: { [K2 in manyprops]: `${K}.${K2}` } }` is not generic,
so the route resolves and prints all 457,000 members when the node is
evaluated. Native resolves a mapped type's members when they are first read
(`resolveStructuredTypeMembers`). Its CLI check never reads them, and its
`.types` print stops at the node builder's truncation length. The base port
is fast here only because it printed the written text (60 characters, also
WRONG).

**What would unblock it.** Publish a non-generic mapped type before its
members are resolved, and resolve them on first read, as the generic arm's
`resolve_mapped_type_members` already does. The obstacle is that a type's
text is fixed when the type is minted (`TypeData::Named`), and the members
print *is* the text. So this needs printing that is deferred, or that
truncates in the node builder's way (`checkTruncationLength`). That is a
printing-architecture change, beyond this lane. The Ir increase on
domain-model (+0.54%) has the same cause: non-generic mapped nodes are now
resolved eagerly.

## 5. A mapped instance iterates its own parameter clone (`tsr-2zk.1053`, committed)

instantiateAnonymousType's mapped arm (`checker.go:22461`) gives every
instance a fresh clone of the declared iteration parameter
(`cloneTypeParameter(getTypeParameterFromMappedType(t))`). The clone's
mapper is `P -> P'` combined with the instance's mapper, so
getConstraintOfTypeParameter(P') is the declared constraint instantiated:
for `MyMap<U>` over `type MyMap<T> = { [P in keyof T]: T[keyof T] }`, that is
`keyof U`, not `keyof T`. The port shared the declared `P` across every
instance. That is why r5-relater5 had to decline an indexed-access relation
through one (`is_mapped_iteration_parameter`, r5-relater5.md "Three
declines"): `U[P] -> U[keyof U]` read `keyof T -> keyof U`.

The port has two roads to an instance, and both now clone:

- **`instantiate_mapped_type_worker`** (a mapper over a captured mapped
  type). The clone is registered in `instantiated_type_parameters` with the
  combined mapper. The template and `as` clause are instantiated under that
  mapper.
- **`mapped_type_info` under alias-evaluation frames.** This road
  re-resolves an alias body under its arguments where native instantiates
  one type, and it is the road `MyMap<U>` takes. The frames are the clone's
  mapper (`alias_evaluation_map`). The template and `as` clause are resolved
  **once**, with `P` bound to the clone in a frame of its own.

**Rejected: renaming afterwards.** The first version resolved the template
under the frames and then instantiated it with `P -> P'`. On
`mappedTypeAsClauseRecursiveNoCrash1` that ran out of memory: 6.5 GiB, then
the watchdog. Its `as` clause names `FlattenType<Source[Key], Target>`, so
the rename re-resolved that alias reference, whose own evaluation renamed
again: two evaluations per level, exponential in the recursion depth.
Binding the clone in a frame resolves each reference once.

**Rejected: a fresh clone per evaluation.** With the frame but no cache, the
same case took 1,854 ms (base 101 ms), over the slow-case ratio. Every
evaluation of the same node under the same frames minted a new `P'`, so
every downstream instantiation missed its cache. Native clones inside an
instantiation that getObjectTypeInstantiation caches by the outer type
arguments, so it makes one clone per instantiation. The clone is therefore
published in `instantiated_objects` under `(P, map)`: `P` stands for its
mapped declaration, and that table's other keys are object types. With the
cache the case takes 158 ms.

**Measured** against `07fadbd`, both dumps unfiltered: types **+4 RIGHT**
(`mappedTypeAsClauseRecursiveNoCrash1:13/15`, `recursiveMappedTypes:11`,
`excessPropertyChecksWithNestedIntersections:106`), diagnostics unchanged,
zero losses, no slow cases. `mappedTypeParameterConstraint` stays EMPTY_RIGHT.

**The relater half (r5-relater6's file).** With this change, the decline is
no longer needed.
[`r5-mapped4-relater-mapped-iteration.diff`](r5-mapped4-relater-mapped-iteration.diff)
removes the decline and `is_mapped_iteration_parameter`. Measured on top of
this commit, both dumps unfiltered: zero change and zero losses, including
`mappedTypeParameterConstraint`, which now relates `U[P'] -> U[keyof U]`
through `P'`'s `keyof U`. Sent to r5-relater6.

### Ownership and work boundaries (checker port convention)

- **Native operations:** instantiateAnonymousType's mapped arm
  (`cloneTypeParameter`, `combineTypeMappers`, the clone's `mapper`);
  getConstraintOfTypeParameter of an instantiated parameter;
  getObjectTypeInstantiation's per-arguments cache.
- **Key identity and owner:** the clone is keyed by
  `(declared P, mapper pairs)` in `instantiated_objects`. For the alias road,
  the pairs are the frames sorted by symbol (`alias_evaluation_map`). Its
  mapper is in `instantiated_type_parameters`, and its symbol is the declared
  `P`'s (`type_parameter_symbols`), as signature clones do.
- **Publication states:** the clone is published the first time an
  instantiation is built, before its template is resolved, so a recursive
  reference inside the template that reaches the same instantiation sees the
  same clone.
- **Receiver/alias context:** the clone's constraint is resolved at query
  time from the declaration's constraint (cached per active frames by
  `type_parameter_constraint`), then instantiated by the clone's mapper.
- **Expensive work boundary:** the instance road adds one map entry. The
  alias road adds one frame push per mapped-node evaluation. Bench Ir is in
  the commit message.

**Falsifier.** Two instantiations that native distinguishes but that share
`(P, map)` here, such as two mapped declarations sharing one parameter
symbol, would share a clone. Each mapped node declares its own parameter,
so this cannot happen for written nodes.

Perf (vs `07fadbd`'s binary): median CPU new/old domain-model 1.001
(21 samples); generic-imports 1.033 at 21 samples and 1.018 at 41;
`diagnostics_match: true`. Callgrind Ir: domain-model 1,195,758,647 →
1,196,776,671 (+0.085%), generic-imports 342,882,571 → 342,931,413
(+0.014%).

## 6. `reducibleIndexedAccessTypes`: `(Payload & { dataType: K })["data"]` (root cause; not ported)

```ts
enum Type { A, B, C }
type Payload = PayloadA | PayloadB | PayloadC   // each { dataType: Type.X; data: … }
type MappedPayload2 = {
    [K in Type]?: (data: (Payload & { dataType: K })["data"]) => void
}
```

Native keeps the template's `(Payload & { dataType: K; })["data"]` deferred
(type row `0:19`). It answers `string` for the `Type.A` member (rows `0:22`–`0:33`). The
port answers `string | number | { x: number; y: number; }` everywhere: the
whole value union. Before the enum-keys diff (`ee6fcb7`), `{ [K in Type]: … }`
had no members, because enum keys vanished, so only row `0:19` could differ.
The member lines appeared with the keys.

**Root cause: `shouldDeferIndexedAccessType`'s reducible arm is not
ported.** The index `"data"` is not generic, and the object
`(PayloadA & { dataType: K }) | …` is not a generic *object* type: no
constituent is instantiable, a generic mapped type or a generic tuple. Native
defers anyway, through the third disjunct (`checker.go:27370`):

```go
return c.isGenericObjectType(objectType) && !(…tuple…) || c.isGenericReducibleType(objectType)
```

`isGenericReducibleType` (`checker.go:24932`) holds for a union containing
intersections where some intersection `isReducibleIntersection`
(`:24937`): instantiated with `uniqueLiteralMapper`, which maps every type
parameter to `uniqueLiteralType`, `getReducedType` changes it. Here
`dataType: Type.A & uniqueLiteral` is `never`, a discriminant with a never
type, so the intersection reduces. `uniqueLiteralType` (`checker.go:1015`)
is a `never` that union reduction treats as a literal, and that
`createUnionOrIntersectionProperty` does not count as `HasNeverType`
(`:21621`). So `{ dataType: K }` alone does not reduce, and only a conflict
with another constituent does. Deferred, the instance's
`K := Type.A` reaches `getIndexedAccessType` with a reducible object, and
`getReducedType` keeps only `PayloadA & { dataType: Type.A }`, whose `data`
is `string`.

The port's `resolved_indexed_access_type` (`indexed.rs`, main's) defers on
`indexed_access_object_is_generic || indexed_access_index_is_generic` only.
It projects through the apparent union at once, before `K` is known.

**What the port needs, and where:**
1. `uniqueLiteralType`: a distinct `never` intrinsic (`intrinsics.rs`).
   `intersection_has_never_discriminant` (`flow.rs`, main's) must not count
   it as never.
2. `is_reducible_intersection` / `is_generic_reducible_type`: instantiate
   with every mentioned type parameter mapped to it, then
   `get_reduced_type` (`intersections.rs`, this lane's; useless without 3).
3. The deferral arm in `resolved_indexed_access_type` (`indexed.rs`) and the
   expression road's equivalent.

Not built here: two of the three pieces are in files this lane may not
touch, and piece 2 alone is dead code. Expected effect: the six
`reducibleIndexedAccessTypes` rows. The other users of
`isGenericReducibleType` (`shouldDeferIndexType`'s union arm,
`checker.go:26838`, which `getIndexType` uses) are part of the same port.

## 7. Item 3: conditional nodes and `keyof` operand shapes (`.16.71`, `.16.100`, `.16.99`)

The brief places these after diff 1 (§4), which is held. Only the part that
does not depend on it was done.

**`keyof` operand parentheses (`.16.99`, committed half + measured diff).**
`emitTypeOperator` emits its operand at `TypePrecedenceTypeOperator`
(`printer.go:2274`), so a union, intersection, conditional or function
operand is parenthesised: `keyof (keyof T extends never ? … : …)`. The port
had two partial rules. `mapped.rs` `mapped_type_text` wrapped by flags (a
union or intersection, including an aliased union, which prints as a
reference and takes no parentheses). `declared.rs`' deferred `keyof` mint
wrapped only a deferred intersection. Both now ask
`node_reuse::binds_below_type_operator`, the §1 text reader at the
`TypeOperator` rung.

- `mapped.rs` (committed): zero verdict change, zero losses on both dumps,
  no slow cases. Ir domain-model 1,196,134,031 → 1,196,108,889,
  generic-imports 342,926,358 → 342,906,222 (both −0.01%); median CPU
  new/old 1.021 and 1.028 (21 samples).
- [`r5-mapped4-keyof-operand-parens.diff`](r5-mapped4-keyof-operand-parens.diff)
  (`declared.rs`, r5-declared's): measured on top, both dumps unfiltered,
  types **+4** (`controlFlowGenericTypes:298/300/301/303`), diagnostics
  unchanged, zero losses, no slow cases.

**Not done, with hypotheses** (the `.16.99` target cases at `22f35b4`):

- `keyofAndIndexedAccessErrors:10–13`, `keyof` of a primitive or boxed
  primitive answering `any`. getIndexType's default arm is
  `getLiteralTypeFromProperties` over the apparent type. The `keyof` arm in
  `declared.rs` routes only object, mapped and type-parameter operands (plus
  `any`/`never`/`unknown` in diff 1) to `resolved_keyof_type`.
- `formatToPartsFractionalSecond:13/15`: the origin prints
  `keyof DateTimeFormatPartTypesRegistry`, where native prints
  `keyof Intl.DateTimeFormatPartTypesRegistry`. The origin text is
  `type_to_string(target)` with no symbol chain (`tsr-2zk.39`).
- `keyRemappingKeyofResult`, `inferenceUnionOfObjectsMappedContextualType`,
  `emptyObjectNotSubtypeOfIndexSignatureContainingObject1/2`,
  `variadicTuples1`: not traced. Their WRONG rows are not `keyof` prints
  (`Oops` vs `"str"`, `any`/`error` results, tuple element widening).

**`.16.71`/`.16.100` (conditional nodes, mapped nodes under alias
bindings): not started.** Both rewrite the same `declared.rs` arm as diff 1
(the written-text mint) and the conditional evaluator, so they wait on §4.
