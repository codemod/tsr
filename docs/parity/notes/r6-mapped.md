# r6-mapped — generic mapped instances, `.16.8`/`.16.91` classification (`tsr-2zk.16.8`, `.16.91`, `.16.100`, `.16.71`)

Round-6 cloud lane, successor to r5-mapped6 (`r5-mapped6.md`). Owns
`crates/tsr-checker/src/mapped.rs`, `intersections.rs`, `intrinsics.rs`
and this note. Other lanes' and main's files ship as measured diffs.
Native source is `vendor/typescript-go` @ `5b1047d`; every expectation
below was checked against a native probe built with
`scripts/offline-cargo/build-tsgo.sh`.

## 0. Baseline and setup

Frozen at `claude/beautiful-shannon-ar5gh0` `b18aec06` (main `17265fac`
plus bookkeeping), which carries r5-mapped6's route diff.

- types 549,853 RIGHT / 5,607 WRONG / 843 GAP (556,303 lines);
- diagnostics 5,530 RIGHT + 5,596 EMPTY_RIGHT (1,063 WRONG, 49
  EMPTY_WRONG) of 12,238;
- Ir (`valgrind --tool=callgrind`, release `tsr`, `--singleThreaded
  --pretty false --noEmit`): domain-model 1,091,471,488; generic-imports
  343,082,464.

Setup as r5-operators3 §4: PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept outside the repo
(`parse` is `tomllib.loads`, `inline_table` a marked `dict`, `dumps` a
small table writer).

## 1. A generic mapped instance prints its mapped form (committed + diff)

**Forcing constraint.** `dependentDestructuredVariablesFromNestedPatterns`
calls `Promise.allSettled(fn())` with `fn: () => T` and
`T extends readonly unknown[]`. The lib signature returns
`{ -readonly [P in keyof T_1]: PromiseSettledResult<Awaited<T_1[P]>>; }`;
under `T_1 := T` native prints
`{ -readonly [P in keyof T]: PromiseSettledResult<Awaited<T[P]>>; }`. The
port printed `ReadonlyArray`'s members (`[x: number]: …; at: …; concat: …`).

**Native.** instantiateMappedType (`checker.go:22535`) maps the homomorphic
variable `T_1` to `T`, a type parameter, which is neither an array nor a
tuple, so it answers instantiateAnonymousType: a MappedType whose
constraint is `keyof T`. createTypeNodeFromObjectType
(`nodebuilderimpl.go:2690`) prints a type that `isGenericMappedType`
(`checker.go:24908`) holds for through createMappedTypeNodeFromType, from
its parts. Its members are resolved only when read.

**Cause.** `instantiate_mapped_type_worker` (`mapped.rs`) always answered
`resolved_mapped_object`, the member print, whether or not the instance
was still generic. `evaluate_mapped_type_node`, the node's own route,
already asks `is_generic_mapped_info` first.

**Ported (commit).** The worker asks `is_generic_mapped_info` before the
member print, and mints a generic instance from `mapped_type_text`, as
the node route does. The commit also records the instance's target and
mapper on `MappedTypeInfo::instance` (native's `MappedType.target` and
`.mapper`), and adds `apparent_mapped_instance` and
`is_generic_mapped_type`, which only the diff below reads (each carries an
`allow(dead_code)` naming the diff).

**Measured** (commit alone, both dumps unfiltered, against the frozen
base):
- types **+4 RIGHT**: `dependentDestructuredVariablesFromNestedPatterns:14/15/16/25`;
- diagnostics unchanged; zero losses on both dumps; no base-RIGHT key
  missing; slowcases clean on both dumps;
- Ir: domain-model 1,091,471,488 → 1,091,707,823 (+0.022%),
  generic-imports 343,082,464 → 343,063,422 (−0.006%);
- median child CPU new/old: domain-model 0.970 (41 samples);
  generic-imports 1.065 then 0.992 at 41 samples, with a base-vs-base
  control at 1.011, so the first reading was noise. `diagnostics_match:
  true` throughout.

### The diff: getResolvedApparentTypeOfMappedType for an instance

[`r6-mapped-apparent-instance.diff`](r6-mapped-apparent-instance.diff),
`mapped.rs` and `contextual.rs` (main's), on top of the commit.

**Forcing constraint.** The rest of the function reads `promises.map`.
Native's getApparentType (`checker.go:21729`) applies
getResolvedApparentTypeOfMappedType to a generic homomorphic mapped type:
when every constituent of the modifiers type's base constraint is an array
or tuple, the apparent type is `instantiateType(target,
prependTypeMapping(typeVariable, baseConstraint, mapper))`, here
`PromiseSettledResult<unknown>[]`. So `map`'s callback reads
`PromiseSettledResult<unknown>`, and the 11 lines under it follow. The
port's `apparent_mapped_type` had this only for an alias reference
(`type_reference_targets`); a signature's mapped return has none.

- **`mapped.rs` hunk:** `apparent_mapped_type` falls back to
  `apparent_mapped_instance`, which re-instantiates
  `MappedTypeInfo::instance`'s target (or the type itself, for a
  non-instance) with `typeVariable := base` prepended to its mapper. The
  type variable is the target's deferred `keyof` operand when it is a type
  parameter, and an `as` clause declines, as getHomomorphicTypeVariable and
  the `NameType == nil` test do.
- **`contextual.rs` hunk:** that alone loses 4 lines
  (`typeParameterConstModifiersReverseMappedTypes:49/50/52/53`, `test5`'s
  `...args: { [K in keyof T]: T[K] }` with `const T extends readonly
  unknown[]`). `contextual_argument_type` read the rest type's property
  `"0"` through `contextual_type_for_element_expression`, which now sees
  `readonly unknown[]` and answers `unknown`, so the argument loses its
  const context. Native never reads that property:
  getContextualTypeForArgumentAtIndex (`checker.go:29772`) answers
  `getIndexedAccessType(restType, numberLiteral)`, which defers for a
  generic object, and isConstTypeVariable reaches `T` through the indexed
  access and isConstMappedType. The hunk sends a generic mapped rest type
  to `resolved_indexed_access_type`, the arm a type-parameter rest type
  already takes. It also removes the two `allow(dead_code)`s.

**Measured** with the diff (commit + diff, both dumps unfiltered, against
the frozen base):
- types **+19 RIGHT** (the commit's 4 and 15 more, all
  `dependentDestructuredVariablesFromNestedPatterns`: lines 23, 24, 26–29,
  31–33, 35–38, 40, 47); zero losses; diagnostics unchanged; slowcases
  clean.
- The lines that stay WRONG in that case (53, 54, 56–58) print
  `readonly [...]` for an `as const` tuple under a contextual mapped type,
  a different cause, not traced.

### Ownership and work boundaries (checker port convention)

- **Native operations:** instantiateMappedType / instantiateAnonymousType
  (`checker.go:22535`, `:22458`); createTypeNodeFromObjectType's
  isGenericMappedType arm (`nodebuilderimpl.go:2690`);
  getResolvedApparentTypeOfMappedType (`checker.go:21772`, diff); isConstTypeVariable (`:13645`).
- **Key identity and owner:** unchanged. An instance is cached in
  `instantiated_objects` under `(target, map)`, as before; its
  `MappedTypeInfo` is owned by `mapped_types` under the instance's id.
  `MappedTypeInfo::instance` is a value on that info (the target id and a
  copy of the mapper), not a new table.
- **Publication states:** a generic instance is published complete when
  minted, with no member table; members are resolved on first read by the
  existing `resolve_mapped_type_members`, as for a node-built generic
  mapped type. The apparent type is memoized in `mapped_apparent_types`,
  as the alias arm already was.
- **Receiver/alias context:** the instance's mapper is the one it was
  instantiated with; `apparent_mapped_instance` prepends the homomorphic
  variable to that mapper, so another instance of the same target has its
  own apparent type.
- **Expensive work boundary:** the commit removes work, the eager member
  resolution and print of a generic instance. The diff adds one
  instantiation of the target per generic homomorphic instance whose
  apparent type is asked for and whose constraint is array-like.

## 2. `.16.100`: a mapped node under alias bindings maps a tuple image elementwise (committed)

**Forcing constraint.** `mappedArrayTupleIntersections:11`:
`type Hmm<T extends any[]> = T extends number[] ? MustBeArray<{ [I in keyof T]: 1 }> : never`,
and `Hmm<[3, 4, 5]>`. Native prints `[1, 1, 1]`; the port printed the
tuple's members (`{ [x: number]: 1; 0: 1; 1: 1; 2: 1; concat: 1; … }`).

**Native.** getConditionalType instantiates the true branch with the
alias mapper (`checker.go:24300`), so the mapped node's declared type,
a MappedType with homomorphic variable `T`, goes through
instantiateMappedType (`:22535`). Its `mapTypeWithAlias` over `T`'s image
`[3, 4, 5]` reaches instantiateMappedTupleType, one template
instantiation per element.

**Cause.** The port has no declared generic type for the node. It
re-evaluates the node under the alias-evaluation frames (r5-mapped4 §5),
so `keyof T` resolves to the tuple's keys and the members print. The alias
road (`instantiate_mapped_alias_sequence`, for `Boxify<[…]>`) already
takes instantiateMappedType's sequence arms; a node written inline in a
branch did not.

**Ported.** `evaluate_mapped_type_node`, under alias frames and for a
homomorphic node, asks `instantiate_mapped_sequence` first, the same
function the alias road uses. Its `replace_source` re-evaluates the node
with the homomorphic variable rebound to one constituent, in a frame of its
own, which is the port's `prependTypeMapping(typeVariable, t, mapper)`.
That covers native's arms for a primitive image (answered as is), a union
image (distributed), an array or array intersection, and a tuple. Outside
alias frames the variable is the declared type parameter, which no arm
admits, so nothing else changes.

**Measured** (on top of §1's commit, both dumps unfiltered, against the
frozen base):
- types **+1 RIGHT** (`mappedArrayTupleIntersections:11`); no other
  line's text changes; diagnostics unchanged;
- zero losses; slowcases clean;
- Ir against §1's commit: domain-model 1,091,707,823 → 1,091,671,012
  (−0.003%), generic-imports 343,063,422 → 343,092,245 (+0.008%);
- median child CPU new/old against the frozen base (21 samples):
  domain-model 0.971, generic-imports 0.963. `diagnostics_match: true`.

### Ownership and work boundaries (checker port convention)

- **Native operation:** instantiateMappedType's `mapTypeWithAlias` arms
  (`checker.go:22535`), instantiateMappedTupleType and
  instantiateMappedArrayType.
- **Key identity and owner:** the result is published in
  `type_literal_types` under the node's `type_literal_key`, as a built
  mapped node already was. Each constituent's re-evaluation runs under a
  frame that binds the variable to that constituent, so it has its own key.
- **Publication states:** unchanged; an `error` constituent declines the
  node, and a declined node is not published (r5-mapped6 §1).
- **Receiver/alias context:** the frame pushed for a constituent sits on
  top of the caller's frames and is popped before the result is published.
- **Expensive work boundary:** one template instantiation per tuple
  element instead of the tuple's whole member table and its print.

## 3. `.16.8` / `.16.91` / `.16.100`: what is left, classified against native

Every remaining non-RIGHT line of `.16.91`'s nine cases and `.16.100`'s
eight, and the mapped-shaped lines of `.16.8`, was probed against the
native build. None of what is left is in this lane's files except where
noted.

**(a) declared.rs `get_instantiated_type_reference`'s alias-declared
`return error` (r6-declared).** A reference to a conditional alias in an
alias-declared position that `evaluate_conditional_alias` cannot evaluate
answers `error` (`declared.rs`, the `§36`/`§92.1` arm, "Upstream evaluates
conditional aliases in alias-declared positions even through
type-parameter arguments"). Native defers it: getTypeAliasInstantiation
gives a ConditionalType that prints as the alias reference. Inside a
mapped node that `error` makes `mapped_type_info` decline, so the node
falls back to its written text. Probed, outside an alias body the same
nodes already print as native does. Lines:
- `mappedTypeAsClauses:72/98/100/107/108` (as-clauses `Extract<P, …>`,
  `Exclude<…>`, `If<…>`);
- `recursiveMappedTypes:24` (`Remap2<T[P]>` in the template);
- `reactReduxLikeDeferredInferenceAllowsAssignment:82`
  (`HandleThunkActionCreator<TDispatchProps[C]>`);
- `conditionalTypes2:172–184`.

*Measured experiment* (not shipped; it is r6-declared's file and it has
losses): letting that arm fall through to the named reference, on top of
§2's commit, gives types **+78 / −21** and diagnostics −1 case
(`contextualTypesNegatedTypeLikeConstraintInGenericMappedType3`,
EMPTY_RIGHT → EMPTY_WRONG). Gains: `conditionalTypes1` ×33,
`mappedTypesArraysTuples` ×8, `conditionalTypes2` ×8,
`intersectionWithIndexSignatures` ×7, `mappedTypeAsClauses` ×4,
`propTypeValidatorInference` ×4, and 14 more. The losses show what the
`error` stands in for:
- lines where native itself answers `any` from errorType (the circular or
  too-deep `Awaited<BadPromise>`, `BuildTuple<…>`, `_Flatten<…>`, and
  `conditionalTypes1:171/173/189/195`), which the fallthrough prints as the
  alias reference;
- `intersectionWithIndexSignatures:35…57`: native names the result `s`
  (the declaring alias), the fallthrough prints `constr<{}, …>`;
- `contextualTypesNegatedTypeLikeConstraintInGenericMappedType3` ×5: the
  deferred reference reaches contextual typing and widens a parameter to
  `number | Event`.

So the faithful change is a deferred conditional with its alias identity,
plus native's TS2589/circularity `errorType` for the others, not a plain
fallthrough.

**(b) Cross-file alias accessibility (r6-printer).** Native prints an
alias only when `IsTypeSymbolAccessible` holds at the enclosing
declaration (`nodebuilderimpl.go:3362`), and otherwise expands it. The
port bakes the alias text at mint. Lines:
- `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1/2/3` ×18:
  the non-exported `Id<…>` in `index.ts`;
- `declarationEmitInlinedDistributiveConditional` ×12;
- `mappedTypeGenericInstantiationPreservesHomomorphism` ×5.

**(c) Unique-symbol keys (r6-declared's `unique_symbols.rs`, r6-printer).**
`declarationEmitMappedTypeTemplateTypeofSymbol` ×6:
`{ [TKey in typeof timestampSymbol]: true }` resolves to one property named
by the symbol, printed `[timestampSymbol]` or `[x.timestampSymbol]` by the
symbol chain. The port's `unique symbol` type does not know its symbol's
printable chain (it prints `unique symbol`, where native prints
`typeof timestampSymbol`). Spelling the key from the declaration's name
would be a guess for `Symbol.iterator`-style keys, so `mapped.rs` keeps
declining the key.

**(d) A deferred conditional template under alias frames (`.16.71`'s
domain, declared.rs).** `declarationAssertionNodeNotReusedWhenTypeNotEquivalent1:16/17`:
with (a) relaxed, the mapped node builds, but its template's written-text
conditional is instantiated per key outside the frames, so `T` stays
unsubstituted.

**(e) Other owners.**
- `deeplyNestedMappedTypes:68/93/124–134`: `PropertiesReduce<…>` is an
  alias whose body is another alias reference, left unexpanded
  (declared.rs alias instantiation);
- `typeParameterConstModifiers` ×9: union and const inference
  (inference.rs);
- `mappedTypeRecursiveInference2` ×28: reverse-mapped inference of a
  recursive tuple (inference.rs);
- `conditionalTypes2:39/47/139`: `T_1` renaming of a signature's type
  parameter (printing);
- `reverseMappedTypeIntersectionConstraint` ×8: `any` from reverse-mapped
  inference over an intersection constraint (inference.rs);
- `dependentDestructuredVariablesFromNestedPatterns:53–58`: an `as const`
  tuple under a contextual mapped type keeps `readonly`.

## 4. TS2540 on a mapped member (diff)

[`r6-mapped-readonly-members.diff`](r6-mapped-readonly-members.diff),
`readonly_target.rs` (no round-6 owner; the integrator routes it) and a new
test `tests/mapped_readonly_members.rs`.

**Forcing constraint.** `omitTypeHelperModifiers01` (`Omit<A, 'a'>`'s
`x.c = true`, `A.c` readonly) misses native's TS2540. Probing showed it is
not `Omit`-specific: `Pick<A, 'c'>`, a local `MyPick` and the plain
homomorphic `{ [P in keyof A]: A[P] }` all miss it, though the last prints
`{ a: number; readonly c: boolean; }`. `globalThisReadonlyProperties`,
the other case r5-smallcodes3 listed, is already RIGHT on the base.

**Native.** resolveMappedTypeMembers (`checker.go:20894`) gives each
mapped symbol CheckFlagsReadonly from the `readonly` modifier or its
modifiers property, and isAssignmentToReadonlyEntity
(`checker.go:27279`) reads isReadonlySymbol on the property
getPropertyOfType answers.

**Cause.** `is_assignment_to_readonly_property` returned `false` as soon
as `get_property_of_type` answered `None`, and it does for a resolved
mapped member (no binder symbol stands for native's transient mapped
symbol). The member image's `readonly`, which the function already reads
for const-context literals, was never consulted. For an alias instance
(`Omit<A, "a">` is a reference whose body is the `Pick` instance) the
image is on the body, resolved on first read.

**The hunk.** The image is read first, on the receiver and on
`binding_type_alias_body(receiver)` after `resolve_mapped_type_members`;
with no property symbol, the image decides. The constructor permission
after it needs a declared property, which a mapped member is not, so it is
skipped in that case only.

**Measured** on top of §2's commit, both dumps unfiltered:
- diagnostics **+1 case** (`omitTypeHelperModifiers01`, WRONG → RIGHT);
- types **+2 RIGHT** (`omitTypeHelperModifiers01:27/29`: the type road's
  readonly write target prints `any`, as native's does);
- zero losses on both dumps; slowcases clean;
- Ir against §2's commit: domain-model +0.007%, generic-imports −0.002%.

The test fails without the hunk (no TS2540 at all) and matches the native
probe line for line, including the two silent lines (`p5.a = 1`, and the
non-homomorphic `{ [P in 'c']: A[P] }`).

## 5. `.16.71`, re-measured on this lane's stack

r6-declared landed getObjectTypeInstantiation's referenced-parameter
keying (`f9339d0`) and re-measured r5-mapped6's
[`r5-mapped6-conditional-typed-print.diff`](r5-mapped6-conditional-typed-print.diff)
there at +5, 0 losses (r6-declared.md §1.1). On this lane's two commits
plus `f9339d0`, both dumps unfiltered:
- types **+5 RIGHT, zero losses**, the same five lines
  (`complicatedIndexesOfIntersectionsAreInferencable:5`,
  `simplifyingConditionalWithInteriorConditionalIsRelated:17/19`,
  `wideningWithTopLevelTypeParameter:47`, `mappedTypeAsClauses:106`);
  diagnostics unchanged; slowcases clean;
- Ir: domain-model 1,090,509,192 → 1,092,174,496 (**+0.153%**),
  generic-imports −0.001%.

**Where the +0.153% goes.** The profile difference is
`conditional_type_text` (2.08 M inclusive): the deferred conditional's text
is printed at mint from `get_type_from_type_node` of all four parts, so
both branches are evaluated when the type is created. Native evaluates the
check and extends types at creation, but instantiates the branches only
when they are read (getTrueTypeFromConditionalType), which for a print is
the node builder. This is the same constraint as §6: the port prints at
mint. The diff's halves go to their owners as the brief says: the
`declared.rs` half to r6-declared, the `node_reuse.rs` half
(`binds_below_union`, `binds_below_function`) to r6-nodereuse. Landing it
needs either the integrator's acceptance of +0.15% for +5, or lazy text.

## 6. ADR-0050 alternative 1, lazy mapped text: not done

Measured on domain-model at §2's commit, inclusive Ir:
`resolved_mapped_object` 6.5 M, of which `mapped_object_text` 5.1 M
(0.47%; its `mapped_property_member` reads 4.8 M, mostly
`get_type_of_mapped_symbol` instantiating each member's template for the
print). That is the eager member print of every non-generic mapped type at
mint, including lib's `{ [K in keyof any[]]?: boolean }`.

Removing it needs the type's text computed when it is first printed. Every
`type_to_string` reads `TypeData::Named`'s baked text through `&self`
(ADR-0050 alternative 1), so this is `printing.rs`/`types.rs` work
(r6-printer and main), not `mapped.rs`. No measurable Ir drop is
available from this lane's files alone: `mapped_object_text` already
truncates and reads each slot once. Not counted as a perf item.

Seen in the same profile, outside this lane: `complete_reverse_mapped_type`
(inference.rs) costs 3.1 M inclusive on domain-model, 1.5 M of it in
`pending_reverse_mapped.remove` (a hashbrown `remove_entry` on a
`ReverseMappedInfo` table).

## 7. Head summary and landing order

**Commits** (on the frozen base `b18aec06`):
- `6e17725`: a generic mapped instance prints its mapped form (§1), +4;
- `5026f57`: a mapped node under alias bindings maps a tuple image
  elementwise (§2), +1;
- `ce372e4`: notes and diffs only (§3–§6).

**Diffs, in landing order**, each measured on top of the commits:

| diff | files (owner) | effect |
|---|---|---|
| `r6-mapped-apparent-instance.diff` | `mapped.rs`, `contextual.rs` (main) | +15 types (§1) |
| `r6-mapped-readonly-members.diff` | `readonly_target.rs` (unassigned), new test | +1 case, +2 types (§4) |
| `r5-mapped6-conditional-typed-print.diff` | `declared.rs` (r6-declared), `node_reuse.rs` (r6-nodereuse) | +5 types after r6-declared's `f9339d0`; Ir dm +0.153% (§5) |

The two commits and the first two diffs together, against the frozen base,
both dumps unfiltered:
- types 549,853 → 549,875 RIGHT (**+22**); diagnostics 5,530 → 5,531
  RIGHT (**+1 case**, `omitTypeHelperModifiers01`);
- zero losses on both dumps; no base-RIGHT key missing; slowcases clean;
- Ir: domain-model 1,091,471,488 → 1,091,820,511 (+0.032%),
  generic-imports 343,082,464 → 343,087,648 (+0.002%);
- median child CPU new/old (21 samples): domain-model 0.970,
  generic-imports 0.981; `diagnostics_match: true`;
- `cargo test --workspace --release` passes with and without the diffs;
  clippy (stable 1.97) flags only pre-existing code in other files;
  `xtask anchors` has only the pre-existing unresolved
  `tsr-conformance/src/full_oracle.rs:4`.

**Remaining, with causes:** §3 (a)–(e). The largest is declared.rs's
alias-declared `return error` (§3 (a), measured +78/−21 as a plain
fallthrough); cross-file alias accessibility is next (§3 (b), 35 lines).
Lazy text (§6) blocks both `.16.71`'s Ir and item 4.
