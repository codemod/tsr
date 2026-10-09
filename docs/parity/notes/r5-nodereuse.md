# r5-nodereuse — the pseudochecker and the shared reuse decisions (`tsr-2zk`)

Round-5 cloud lane. It owns `crates/tsr-checker/src/node_reuse.rs` (moved from
r5-mapped5), the new `crates/tsr-checker/src/pseudochecker.rs`, its tests, and
this note. Every printer call site lives in a file another lane owns:
`signatures.rs`, `printing.rs` and `objects.rs` belong to r5-printer2,
`checker.rs` to main, and `spreads.rs` to no box in this round. Those changes
therefore ship as the measured diffs in §6.

Native source is `vendor/typescript-go` @ `5b1047d`. The functions mirrored
are `serializeTypeForDeclaration` (`checker/nodebuilderimpl.go:2181`),
`serializeReturnTypeForSignature` (`:2023`) and
`typeToTypeNodeHelperWithPossibleReusableTypeNode` (`:1597`). On the
pseudochecker side they are `pseudochecker/lookup.go` and
`checker/pseudotypenodebuilder.go`, and the visitor is
`checker/nodecopy.go`.

Baseline frozen at `e20cdd4`: types 548,751 RIGHT / 900 GAP / 6,640 WRONG;
diagnostics 5,431 RIGHT + 5,590 EMPTY_RIGHT of 12,238 rows.

## 1. The rule table

The lane's targets, re-measured at the base and grouped by the native rule
that decides each one. "Lines" counts WRONG type lines at the base.

| # | native rule | where native decides | targets (lines) | port |
|---|---|---|---|---|
| R1 | **Return slot, pseudo return.** `serializeReturnTypeForSignature` asks `GetReturnTypeOfSignature`. An unannotated value signature with exactly one top-level `return` gives that expression's pseudo type (`typeFromSingleReturnExpression`, `lookup.go:211`), and an assertion's is `Direct(its type node)` (`typeFromTypeAssertion`, `:510`). The node is reused when `getTypeFromTypeNode(node)` is the return type. | `nodebuilderimpl.go:2040` | `baseClassImprovedMismatchErrors` (2), `declarationEmitScopeConsistency3` (2 of 4), `declarationEmitPartialNodeReuseTypeOf` (2), `declarationEmitPartialReuseComputedProperty` (2), `declarationEmitGenericTypeParamerSerialization2` (2), `declarationEmitScopeConsistency` (3) | §2, §3 |
| R2 | **Return slot, written predicate.** The predicate NODE is the Direct pseudo return. It is reused when `pseudoReturnTypeMatchesPredicate` holds (`pseudotypenodebuilder.go:645`): same `asserts`, same target, same type. The written type order survives. | `nodebuilderimpl.go:2045` | `typePredicatesOptionalChaining3` (2), `typeGuardNarrowsToLiteralTypeUnion` (2), `partialTypeNarrowedToByTypeGuard` (2), `typeGuardNarrowsPrimitiveIntersection` (4), `subtypingWithCallSignatures2/3` (30) | §2 |
| R3 | **Constraint.** `typeParameterToDeclaration` reuses `getConstraintDeclaration`'s node when its type is the constraint. Native has one node builder; this port has four signature printers, and the two member printers lacked the arm. | `nodebuilderimpl.go:1611` | `objectFreeze` (8), `objectFromEntries` (4), `circularContextualReturnType` (4), `contextualSignatureInObjectFreeze` (2), `objectFreezeLiteralsDontWiden` (2) | §2 |
| R4 | **Property slot.** `addPropertyToElementList` → `serializeTypeForDeclaration(nil, type, symbol, true)`. The property's declaration gives its Direct pseudo type: an annotation, or an assertion initializer (`typeFromPropertyAssignment`, `typeFromProperty`). That node is reused when equivalent, and an optional annotation is compared with `undefined` stripped (`isOptionalAnnotated`). | `nodebuilderimpl.go:2223` | optional-`\| undefined` (`spreadObjectPermutations` 4, `spreadUnionPropOverride` 2, `thislessFunctionsNotContextSensitive3` 2, `unionExcessPropsWithPartialMember` 2, `declarationEmitComputedPropertyNameEnum2/Symbol1/Symbol2` 6), `destructuringParameterDeclaration10` (4), rest `...rest: any` in a function type (`collisionArgumentsInType` 2, `collisionRestParameterInType`, `emitRestParametersFunctionProperty*` 2), written computed names (`declarationEmitComputedPropertyName1` 2), written member order (`typeName1`), `typeof undefined` (`widenedTypes`), `as` in an object-literal property (`spreadObjectNoCircular1` 2), `null \| 'string'` (`intersectionIncludingPropFromGlobalAugmentation`) | §4 |
| R5 | **Visitor scope** (`nodecopy.go`). A computed property name's entity is tracked at the site, and a failure is `markError` (`:751`), which refuses the whole node. A pseudo node from a body sees only the printed signature's own parameters (`enterNewScope`). | `nodecopy.go:751`, `nodebuilderscopes.go:59` | prerequisite of R1 (kept `declarationEmitScopeConsistency3` ×5 and `declarationEmitPartialReuseComputedProperty` ×3 RIGHT) | §3 |
| — | Structural pseudo types (object literal, single call signature): accessor pairs kept as accessors (`getAccessorMember`, `lookup.go:376`) | `pseudotypenodebuilder.go:409` | `circularObjectLiteralAccessors` (3); `divergentAccessors1` (4) is `addPropertyToElementList`'s accessor arm, a printer rule | not ported (§5) |
| — | Iterable-family default type-argument elision, which is not node reuse | `typeReferenceToTypeNode`, `nodebuilderimpl.go:3085` | `builtinIteratorReturn` (4) | reported (§5) |
| — | Inaccessible alias at a foreign site (`trackExistingEntityName` → `serializeTypeName` → `IsSymbolAccessible`) | `nodecopy.go:436` | `declarationEmitPartialNodeReuseTypeReferences` (6) | open (§5) |
| — | Written mapped-template arguments | mapped template print | `mappedTypeTupleConstraintAssignability` (2) | r5-mapped5 (§5) |

`cannotIndexGenericWritingError` was already RIGHT at the base (r5-mapped4
§2).

## 2. One decision per slot, asked by every printer

The port has four signature printers: `signature_to_string` and
`signature_to_string_at_worker` in `signatures.rs`, `signature_member_text_at`
in `checker.rs`, and `objects::signature_member_text`. Each made its own
reuse choice. Native has one node builder, so a type cannot print differently
depending on which printer reaches it. `Object.freeze`'s overloads reached the
two member printers, which never asked for constraint reuse (R3), so the
written `U | null | undefined | object` was sorted.

The decisions now live in `node_reuse.rs`, one function per native slot, and
every printer asks the same one:

- `Checker::type_parameter_constraint_text`: the carried
  `written_constraint`, else `reused_constraint_text`.
- `Checker::reused_return_text`: the carried `written_return` (an annotation,
  **a predicate node included**), else the pseudochecker's Direct return
  (§3). The predicate gate is `pseudoReturnTypeMatchesPredicate`. A printer
  that gets `None` falls back to the predicate and then to the type, in that
  order. This also changes which arm comes first: the written predicate now
  wins over `type_predicate_to_string` only while it still matches, which is
  native's order.
- `Checker::reused_property_type_text`: R4.

**Not taken: one more per-site patch.** Adding the constraint arm to the two
member printers was a one-line change each, and r5-mapped4 §2 proposed exactly
that. It fixes R3, but it leaves the next slot (returns, predicates) to drift
the same way. The measured cost of drift is the 30 `subtypingWithCallSignatures2/3`
lines: a written predicate that one printer reused and another sorted.

The signature build in `signatures.rs` is unchanged: it still does not carry
a predicate node in `written_return`. `reused_return_text` derives the
written predicate at print time, as native's `createReturnFromSignature`
does for any annotated function-like. An absent carriage on any other
annotation is a refusal the port already made (an alias-mapped node, or an
instantiation that moved the type), and it is not re-derived. The predicate's
own `written_text` (§926's qualified spelling) is still read by
`type_predicate_to_string`, which is now only the fallback.

**The current return type is read as each printer already read it.** A
printer with a site asks `get_return_type_of_signature`, as native's
serializer does. A site-free printer is the one that bakes a type's text at
creation, and it reads `signature.r#type` without demanding a pending return
(`signature_positions`' lazy returns). The first version demanded the return
everywhere. Two unit tests caught it
(`top_signature_demand_follows_structural_admission_not_pending_any_flags`,
`parameter_only_vectors_preserve_original_pending_key_and_decline_an_alias_demand`):
baking a pending signature must not resolve it. While a return is pending,
the slot holds the error placeholder. Error charity would accept a pseudo
node against it, so a body-derived node is refused there. A written
annotation keeps its charity, which is unchanged.

`tests/type_predicates.rs`' `predicates_retain_resolved_mapped_and_indexed_types`
pinned the pre-reuse print of two written predicates
(`x is Box<string>`, `x is keyof T`). Under native's rule the declaration
prints the written node: `x is B<{ a: string; }>["a"]` and
`x is { a: keyof T; }["a"]`. The predicate still has to resolve for the
signature to exist, which was the test's point. The expectation update rides
in the return-slot diff.

**Not taken: carrying the predicate node on the signature.** The first
version of the return-slot diff changed the build so that `written_return`
carried predicate nodes too. It measured the same, but it needed a fourth
foreign hunk. Re-deriving the node at print time also has the property the
pseudo return already relies on: the gate is evaluated against the
signature as it is when printed.

## 3. The pseudochecker, Direct answers only (`pseudochecker.rs`)

Upstream's pseudochecker maps a declaration to one of twenty `PseudoType`
kinds. The port carries the Direct kind (a written type node), because Direct
is the only kind whose print differs from the type's own: the node's spelling,
order and aliases. Keywords and literals print the same either way. The
structural kinds (object literal, single call signature, tuple) are §5.

- `pseudo_direct_return_node` ports `createReturnFromSignature` →
  `typeFromSingleReturnExpression` for an unannotated value-signature
  declaration. `ForEachReturnStatement` visits only native's statement
  containers, and a nested or second return gives up. A contextually typed
  candidate (`isContextuallyTyped`, which does not stop at function
  boundaries) counts only as a non-`const` assertion.
- `pseudo_direct_declaration_node` ports `GetTypeOfDeclaration` for property
  signatures and declarations, property assignments, variables, and
  parameters without an initializer.

**Asked at print time.** Native asks the pseudochecker inside the node builder.
Doing it when the signature is built would walk every unannotated body on the
checking path, and most signatures are never printed. So `reused_return_text`
derives the pseudo node only when a printer asks and nothing is carried. A
carried annotation that an instantiation cleared (`written_return = None`)
is not re-derived: the derivation only runs for a declaration with no return
annotation. Its type gate is the same identity gate the carried annotations
use.

**R5, found by the first measurement.** The pseudo return lost 8 RIGHT lines
in two cases before the visitor fixes:

- `declarationEmitScopeConsistency3`: `const f = (v: "inner") => () => null!
  as typeof v`. The visitor treated a parameter of ANY enclosing function as
  in scope (`declared_inside_reused_node`'s ancestor test). That is right for
  an annotation on the printed signature itself. A pseudo node comes from the
  body, though, and native's `enterNewScope` binds only the printed
  signature's own parameters. `ReuseContext::pseudo_owner` makes the test
  exact for pseudo nodes. Annotations keep the ancestor test, which is
  unchanged.
- `declarationEmitPartialReuseComputedProperty:1:*`: `{ [n]: string }`
  printed in a file that cannot see `n`. The visitor emitted computed names
  without tracking. `nodecopy.go:751` tracks the entity, and a failure is
  `markError`, which refuses the whole node; that refusal is set through
  `ReuseContext::unnameable`. The site-free path now honours `unnameable` as
  well as `scope_local`.

`tsgo --declaration` confirms the scope rule:
`export const g = (v: "outer") => { const f = (v: "inner") => () => null! as
typeof v; ... return r; }` emits `() => "inner"`, while the standalone
`f2 = (v: "inner") => () => null! as typeof v` emits `() => typeof v`.

## 4. The property slot (R4)

`reused_property_type_text(symbol, type, site)` takes the property's value
declaration (else its first declaration) and its Direct pseudo node. It
applies `pseudoTypeEquivalentToType`'s type arms (`pseudo_type_equivalent`):
error charity, identity, the `NEUndefined`-stripped identity when the
annotation is optional, regular-literal identity, and `compareTypesIdentical`
for two unions. The node is then printed through the visitor at the site.

Two printers bake or print property text without the arm. Both are wired in
`r5-nodereuse-property-slot.diff`:

- `printing.rs` `type_literal_text_at`. A property whose annotation is not a
  literal, array or union node was rendered from its type, which sorts the
  `Record<"json" | "jsonc" | "json5", …>` key union and expands
  `...rest` to `any[]`.
- `spreads.rs`' three property producers. Native prints a spread member
  through the original declaration (`owner?: string`). The port printed the
  type, `string | undefined`.

**Not ported:** `requiresAddingImplicitUndefined` is true for a property only
when it is reverse-mapped (`emitresolver.go:593`). This port has no
reverse-mapped symbol for it to read. The `ObjectFlagsRequiresWidening` gate
cannot fail for a Direct node, because the type must be the node's own.

## 5. Remaining, with the reason

- **Structural pseudo types.** Object literals, single call signatures and
  tuples are structural. `circularObjectLiteralAccessors` (3) needs the object
  literal's `getAccessorMember`. `declarationEmitScopeConsistency3:0:3,0:12`
  need the single-call-signature pseudo, where the outer signature's scope
  binds `v` for the inner return. Porting the kinds means comparing a pseudo
  signature with a checker signature
  (`pseudoParametersEquivalentToParameters`). That is the next item for this
  file.
- **`divergentAccessors1` (4)** is `addPropertyToElementList`'s accessor arm:
  it prints the get/set pair when the write type differs. It is a printer rule
  in `printing.rs` (r5-printer2), not reuse.
- **`builtinIteratorReturn` (4)** is `typeReferenceToTypeNode`'s
  Iterable-family default elision (`nodebuilderimpl.go:3085`). It belongs to
  the reference printer (`declared.rs` reference text, r5-declared3).
- **`declarationEmitPartialNodeReuseTypeReferences` (6).** At a foreign site,
  the parameter `p2: PrivateSpecialString` prints the baked text rather than
  the site-aware reuse. `serialize_type_name` would refuse the inaccessible
  alias, but the parameter is printed from text baked in file `a`. That is a
  printer-path question for r5-printer2.
- **`complexRecursiveCollections:1:24,598`** still prints the sorted predicate
  union. A reduced repro reuses the written node correctly, so the corpus case
  reaches another printer path; not chased.
- **`mappedTypeTupleConstraintAssignability` (2):** the mapped template's
  written arguments (`ISchema<T[K], any, any, any>`), mapped.rs (r5-mapped5).

## 6. Measured diffs (call sites in files this lane does not own)

All three depend on the committed `node_reuse.rs` / `pseudochecker.rs` API and
apply in order on top of it.

| diff | files | adds |
|---|---|---|
| `r5-nodereuse-constraint-printers.diff` | `signatures.rs`, `checker.rs`, `objects.rs` | R3: every printer asks `type_parameter_constraint_text` |
| `r5-nodereuse-return-slot.diff` | `signatures.rs`, `checker.rs`, `objects.rs`, `tests/type_predicates.rs` | R1/R2: every printer asks `reused_return_text` first, then falls back to the predicate and the type; the test's written-predicate expectations (§2) |
| `r5-nodereuse-property-slot.diff` | `printing.rs`, `spreads.rs` | R4 at the type-literal printer and the spread producers. **Applies on top of r5-printer2's `1f1fdfd`** (§8): it replaces that lane's `reused_property_annotation_text_at` |

Measurements are in §7.

## 7. Measurements

All against the frozen base `e20cdd4`, both dumps unfiltered, compared with
`cut -f1,2`; `slowcases` on both dumps.

| tree | types | diagnostics | slowcases |
|---|---|---|---|
| owned only (this commit) | 0 gained, 0 lost | 0 gained, 0 lost | clean |
| + constraint-printers | +21 RIGHT, 0 lost | unchanged | clean |
| + return-slot + property-slot (stack, final) | +207 RIGHT (61 cases; 39 cases become fully RIGHT), 0 lost | unchanged | clean |

Converted by the full stack, by case (lines): `subtypingWithCallSignatures2`
21, `assignmentCompatWithObjectMembers` 19, `recursiveTypesWithTypeof` 15,
`subtypingWithCallSignatures3` 9, `objectFreeze` 8, `inKeywordTypeguard(strict=false)` 7,
`recursiveConditionalTypes` 7, `discriminatedUnionTypes4` 7,
`controlFlowOptionalChain` 5, and 52 more cases with 1 to 4 lines each
(the target list of §1 among them). The 39 cases that become fully RIGHT include
`baseClassImprovedMismatchErrors`, `typePredicatesOptionalChaining3`,
`destructuringParameterDeclaration10` (both configurations),
`spreadObjectNoCircular1`, `spreadObjectPermutations` (both),
`collisionArgumentsInType`, `collisionRestParameterInType`, both
`emitRestParametersFunctionProperty*`, `typeName1`, `widenedTypes`,
`intersectionIncludingPropFromGlobalAugmentation`, the
`declarationEmitComputedPropertyName*` four, and
`subtypingWithCallSignatures2/3`.

**Full parity run** (`coverage`): the committed tree gives checker_types
8,358/9,538 (configured 1,670/1,928) and diagnostics 4,589/5,502 (configured
842/1,089). With the three diffs: checker_types **8,391** (+33), configured
**1,676** (+6). Diagnostics are unchanged.

**Perf, median child CPU new/old** (`whole_project_perf.py`, base binary in
the `--tsgo` slot). Owned only: domain-model 0.950, generic-imports 0.974
(21 samples). Full stack, final diffs: domain-model 1.062 at 21 samples,
0.995 at 41; generic-imports 0.978. `diagnostics_match: true` throughout.

**Callgrind Ir** (`--singleThreaded --pretty false --noEmit`, base
1,155,283,424 domain-model / 342,949,134 generic-imports). The base binary
alone varies by 0.08% between two runs (1,155,286,867 and 1,156,217,494).
Each tree below is measured with the earlier diffs applied, so every delta is
that diff's own:

| tree | domain-model | generic-imports |
|---|---|---|
| owned only | +0.05% | −0.00% |
| + constraint-printers | +0.03% | −0.00% |
| + return-slot | +0.11% | +0.01% |
| + property-slot | +0.22% | −0.00% |
| final stack (all three, after the site-free read of §2) | **+0.33%** (1,159,079,113) | −0.00% (342,948,412) |

**Why the property and return slots cost Ir at all.** Native asks
`serializeTypeForDeclaration` and `serializeReturnTypeForSignature` only when
it prints. This port bakes a type's text when the type is CREATED. Its
printers therefore run on the checking path, once per spread result and once
per signature type, whether or not anything is printed. On domain-model, the
property arm runs 1,248 times from `spread_properties`, about 2.7k Ir per
call. Most of that is the visitor's `trackExistingEntityName` resolutions,
three `resolve_name` per entity in site-free mode. The return arm runs about
4.5k times from `signature_to_string`. The lasting fix is to print a spread
member's slot on demand (`PrintedSlot::on_demand` exists for accessors),
which moves the reuse to print time as native has it. That belongs to
`spreads.rs`/`objects.rs`, not this lane.

**Cost cuts kept** (first full-stack measurement +0.95% domain-model Ir):

- `pseudoReturnTypeMatchesPredicate`'s `compareTypesIdentical` fallback is
  asked for two unions only, the way `pseudoTypeEquivalentToType`'s union arm
  asks it, and not for any pair as native does. Every baked instantiation of
  a predicate signature walked the structural identity, which has no relation
  cache in this port (`properties_identical_to`, `identity_property_metadata`).
  That cost 3.9M Ir, and no type line depends on it (the gain set is
  identical). **Falsifier:** a written non-union predicate type whose id
  differs from the signature's predicate type while the two are identical.
  That would print the serialized type where native reuses the node.
- `reused_property_type_text` does not call `getTypeFromTypeNode` while the
  printed type is still the property symbol's own type and the pseudo node is
  that declaration's own annotation. The equivalence then holds by
  construction (the module-doc argument for carried signature annotations),
  and this port re-resolves reference nodes on every call. That saved 2.5M Ir.

**Refused, with the number:**

- A per-(node, owner) memo of the site-free visitor's output (a `Checker`
  field in `checker.rs`). It saved 0.2M of the property arm's 2.6M on
  domain-model: the walked nodes are mostly distinct, so the cost is
  first-time work, not repetition.
- Asking `resolves_from_file_top_level` before `is_global_name` in the
  site-free tracker. It is neutral, because `is_global_name` exits on
  `is_external_module` before resolving anything.

## 8. Rebased onto r5-printer2 (`1f1fdfd`)

r5-printer2 finished without landing these hunks and asked for a rebase onto
its branch. Its `ce7f64d` ported the same native slot in `printing.rs`
`type_literal_text_at`, as `reused_property_annotation_text_at`. That helper
accepts an annotation when its type is the property's, or the property's
with optionality forgiven, and it covers only type-literal annotations.
`ee03332` added an accessor-pair arm in front of it.

The rebased `r5-nodereuse-property-slot.diff` **replaces** that helper with
`reused_property_type_text` and deletes it. The accessor arm stays ahead of
it, as native's `addPropertyToElementList` checks accessors first. The
constraint and return-slot diffs apply to that tree unchanged. r5-printer2's
`r5-printer2-member-constraint-reuse.diff` (`checker.rs`, the member
printer's constraint) is subsumed by the constraint-printers diff; the
integrator lands one of the two, not both.

**Measured** against a baseline frozen at `aa37348`, main `e20cdd4` merged
into r5-printer2's head (types 548,929 RIGHT / 900 GAP / 6,462 WRONG), with
this lane's commits and all three diffs applied:

- types +101 RIGHT, 0 lost; diagnostics 0 lost; slowcases clean;
- `cargo test -p tsr-checker` passes, r5-printer2's
  `property_annotation_reuse` and `optional_parameter_printing` included;
- median CPU new/old (41 samples): domain-model 0.990, generic-imports
  1.011; callgrind Ir domain-model 1,156,985,204 → 1,161,520,733 (+0.39%).

Zero losses against a base that already held the helper's gains (+106 lines
by r5-printer2's count) means the replacement is a superset at verdict
level. The +101 is what remains of this lane's +207 once r5-printer2's own
property and constraint ports are counted in the base.

