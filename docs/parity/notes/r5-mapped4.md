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
