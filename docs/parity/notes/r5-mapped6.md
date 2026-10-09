# r5-mapped6 — the held route's Ir, conditional nodes, keyof of reducible unions (`tsr-2zk.1093`)

Round-5 cloud lane, successor to r5-mapped5 (`r5-mapped5.md`). Owns
`crates/tsr-checker/src/mapped.rs`, `intersections.rs`, new intrinsics in
`intrinsics.rs`, `printing::prints_as_a_single_token`, and this note.
`declared.rs` (r5-declared3) and the other lanes' files ship as measured
diffs. Native source is `vendor/typescript-go` @ `5b1047d`.

## 0. Baseline and setup

r5-mapped5's commits and its six non-route diffs are scheduled for
integration batch AR. At dispatch they were not yet on
`claude/beautiful-shannon-ar5gh0` (head `28648eb`, batch AL). The baseline
is therefore **provisional**: `28648eb` merged with r5-mapped5's head
`0a31b0c`, plus the six non-route diffs applied in landing order
(`generic-mapped-keys-constraint`, `contextual-name-type`,
`reducible-indexed-access`, `unique-literal-flow`, `reducible-keyof`,
`keyof-primitive`). That is the tree batch AR should produce.
The diffs stay uncommitted on this branch, and this lane's commits touch
only its own files.

- types 548,933 RIGHT / 6,481 WRONG / 877 GAP;
- diagnostics 5,440 RIGHT + 5,595 EMPTY_RIGHT (1,151 WRONG, 52 EMPTY_WRONG)
  of 12,238;
- coverage: checker_types 8,378 of 9,538 (configured 1,683 of 1,928);
  diagnostics 4,594 of 5,502 (configured 846 of 1,089);
- Ir (`valgrind --tool=callgrind`, release `tsr`, `--singleThreaded
  --pretty false`): domain-model 1,156,753,113; generic-imports
  342,976,360.

Setup as r5-operators3 §4. PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept in the session
scratchpad.

## 1. The held route: its Ir was a second evaluation of declined mapped nodes (committed + diff)

**The brief's premise.** r5-mapped5 §2 held the declared route
(`r5-mapped5-declared-route.diff`, +86 type lines, +1 case) on domain-model
Ir +0.48%. It attributed the cost to the semantic evaluation of 40
`KeysOfType<ModelNNNLine, number>` bodies. The brief asked how the base
avoids that evaluation, and whether native caches or defers it.

**Reproduced.** On the provisional base the route costs domain-model
1,156,753,113 → 1,162,412,449 (+0.49%), and generic-imports −0.004%.

**What the profile shows.** The `KeysOfType` attribution was wrong.

Two microbenchmarks isolate the core shapes, each with 40 instances:
- `KeysOfType<L, number>`: +0.78 M Ir;
- `DeepReadonly<M>` over a chain of interfaces: **+3.2 M Ir**.

Instrumenting `create_semantic_mapped_type` on the second showed:
- 80 of its calls decline, all under two alias-evaluation frames.
- Each decline is `{ readonly [K in keyof T]: DeepReadonly<T[K]> }` with
  `T := M_i`. The template `DeepReadonly<T[K]>` evaluates to `error` under
  the bindings, so `mapped_type_info` answers `None`.
- The route's arm then falls through to the written-text arm, which calls
  `capture_mapped_type`. That calls `mapped_type_info` again on the same
  node in the same context, and it declines again.

The callgrind call counts agree: `mapped_type_info` 96 calls at the base,
87 + 84 with the route. On domain-model the same split is 291 at the base
and 203 + 123 with the route.

The base never answered `KeysOfType` cheaply: it does the same per-key work
through `resolved_indexed_access_type` → `get_type_of_property_with_this_argument`
→ `get_type_of_mapped_symbol` (320 calls, 6.6 M Ir). The route moves that
work into `resolved_mapped_object`'s member print, and the indexed access
then reads the published slots.

**Native.** getTypeFromMappedTypeNode (`checker.go:24170`) evaluates a
mapped node once per context and stores it in `typeNodeLinks.resolvedType`.
It has no declined build and no second evaluation. The port's decline is
its own artifact. A template it cannot evaluate yet (here a conditional
alias over a generic indexed access) makes `mapped_type_info` answer `None`.
The fix keeps that decline, and only removes the repeat.

**Ported** (`mapped.rs`):
- `evaluate_mapped_type_node` evaluates the node once. It answers
  `MappedNodeType::Built(ty)`, or `Declined(parts)` with the evaluated
  `MappedTypeInfo` when there is one. The generic arm declines when its
  text cannot be printed. The non-generic arm declines when
  `resolved_mapped_object` cannot print the mapped form, and that function
  now hands the parts back (`Result<TypeId, MappedTypeInfo>`).
- `create_semantic_mapped_type` is now a wrapper over it, for the callers
  with no fallback.
- `publish_mapped_type_info` is `capture_mapped_type`'s publication step,
  split out so a caller can publish parts it already holds.
- The `TSR_DBG` debug print left in `resolved_mapped_object` is removed.

The route diff ([`r5-mapped6-declared-route.diff`](r5-mapped6-declared-route.diff),
`declared.rs`, r5-declared3) is r5-mapped5's diff with its mapped arm
rewritten. On `Declined(info)` it mints the written text as the base's
`Some(text)` arm does, and publishes `info` on that image instead of
evaluating the node again. The `.16.108` `keyof any|never|unknown` hunk is
unchanged. The diff is generated against `declared.rs` with r5-mapped5's
`reducible-keyof` and `keyof-primitive` diffs applied, which is batch AR's
`declared.rs`.

The diff also removes the `allow(dead_code)` on `MappedNodeType::Declined`'s
payload in `mapped.rs`. Only the diff's arm reads it, so the commit alone
needs the allowance. The diff also updates one unit test, `tests/index_signature_members.rs`,
which asserted `{ [k: string]: keyof T; }` for an unresolved `T`. The
`.16.108` hunk answers `string | number | symbol` there. The native probe
(`scripts/offline-cargo/build-tsgo.sh`) agrees: its TS2322 for that literal
prints `{ [k: string]: string | number | symbol; }`, because
getIndexTypeEx (`checker.go:26701`) answers stringNumberSymbolType for
`keyof errorType`. r5-mapped5's route diff failed this test too; its note
did not record it.

Without the diff the commit changes nothing observable. The written-text
arm and the no-text arm still call `capture_mapped_type` and
`create_semantic_mapped_type` as before.

### Measured

Route + this commit against the frozen base, both dumps unfiltered:
- types 548,933 → **549,018 RIGHT (+85)**: 83 WRONG→RIGHT, 2 GAP→RIGHT, and
  1 GAP→WRONG;
- diagnostics **+1 case** (`bigintIndex`, WRONG→RIGHT);
- zero losses on either dump, and no base-RIGHT key missing;
- slowcases clean on both dumps.
  `hugeDeclarationOutputGetsTruncatedWithError` is 378 ms / 112 MiB.
- **Byte-identical to the route alone** on both dumps (key, verdict, want,
  got). The fix removes work only.
- Coverage: checker_types 8,378 → 8,390 of 9,538 (configured 1,683 of
  1,928, unchanged); diagnostics 4,594 → 4,595 of 5,502 (configured 846 of
  1,089, unchanged).

Converted lines by case:
- `verbatim-declarations-parameters` ×7;
- `paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized` ×7;
- `assignmentGenericLookupTypeNarrowing` ×6;
- `deeplyNestedMappedTypes` ×5, `bigintIndex` ×5;
- `mappedTypeWithAny`, `hugeDeclarationOutputGetsTruncatedWithError` and
  `declarationEmitMappedTypePropertyFromNumericStringKey` ×4 each;
- `typeGuardNarrowsIndexedAccessOfKnownProperty11`/`12`,
  `mappedTypeModifiers`, `mappedTypeAsClauses` and
  `dependentDestructuredVariablesFromNestedPatterns` ×3 each;
- 18 more cases at 1–2 lines.

Median child CPU new/old at 41 samples: domain-model 0.985, generic-imports
1.001. `diagnostics_match: true`. A base-vs-base control on domain-model
read 1.005. A first 21-sample domain-model run read 1.071 and did not
reproduce.

### Ir, and why the plain number is not the gate's answer here

Plain callgrind, one run per binary:

| binary | domain-model | vs base | generic-imports | vs base |
|---|---|---|---|---|
| base | 1,156,753,113 | | 342,976,360 | |
| route (r5-mapped5's diff) | 1,162,412,449 | +0.49% | 342,961,652 | −0.004% |
| route + this commit | 1,159,835,827 | **+0.27%** | 342,935,705 | −0.012% |

The plain run still reads +0.27%. The line-level profile puts the residual
in hashbrown's probe loop (`control/bitmask.rs:40`, `raw.rs:89–90`, the
SSE2 group match). Every function involved has the same call count as at
the base:
- `get_type_of_property_with_this_argument` 44,471 calls;
- `receiver_signature_kinds`' map `get` 35,042;
- `tuple_element_lists`' `get` 31,598.

Each call simply probes longer. The probe steps (`raw.rs:89–90`) go from
0.32 M to 0.98 M Ir, spread over a dozen unrelated `TypeId`-keyed maps. No
checker map changes size by more than 41 entries (a dump of all 99 maps'
`len`/`capacity` at each file). That pattern is a hash-layout effect: the
route allocates `TypeId`s in a different order, which moves keys between
probe groups.

**The test.** A scratch-only build of each tree adds one dummy type at
pseudo-random points in `TypeStore::push` (about 1 in 64 pushes, by a
seeded hash of the type count). This changes the *relative* ids, which a
uniform shift of all ids does not; a uniform shift keeps every pairwise
difference, and those differences decide FxHash collisions. Domain-model
Ir:

| seed | base | route | route + commit |
|---|---|---|---|
| 0 (no jitter) | 1,156,351,166 | 1,161,887,176 | 1,159,510,617 |
| 11 | 1,156,847,499 | 1,158,963,989 | 1,156,550,257 |
| 23 | 1,157,187,422 | 1,159,232,061 | 1,156,121,169 |
| 37 | 1,156,932,241 | 1,160,023,660 | 1,157,553,609 |
| 51 | 1,157,917,084 | 1,160,078,460 | 1,157,027,272 |
| 77 | 1,157,664,952 | 1,159,906,610 | 1,156,777,396 |
| mean of 11–77 | 1,157,309,840 | 1,159,640,956 (+0.20%) | 1,156,805,941 (**−0.04%**) |

What the table shows:
- The layout term alone spans 1.07 M Ir on the base across five seeds,
  0.09%. The commit's seed-0 run sits 2.4 M above its own seeded mean.
- Taken over seeds, the route costs +2.3 M (+0.20%) of real work.
- With this commit the route is 0.5 M *cheaper* than the base. The second
  evaluation it removes was 2.8 M.
- The `(no jitter)` row is from the jitter builds, whose binaries differ
  slightly from the plain ones above.

An earlier uniform-shift experiment added K types before the first real
one, with K ∈ {0, 1, 3, 7, 13, 29, 64, 101, 300, 499, 1000, 1499}. It
spread only 1.17 M on the base. It kept the route + commit build 1.9–4.7 M
above the base at every K (mean +3.5 M). That is the pairwise-difference argument above measured: a
uniform shift does not move keys between probe groups.

**Decision asked of the integrator.** The ±0.1% Ir gate cannot separate
work from layout on a change that allocates types in a new order. On
domain-model the layout term alone is about ±0.1%. I ship the route with
the seeded measurement as its evidence, and do not tune for a lucky plain
run. If the plain number must be within ±0.1%, the remaining options are
not a port of anything native does:
- reorder allocations until the probes happen to fall well;
- change the hasher of the `TypeId` maps (`rustc_hash`), a cross-cutting
  `perf_links`/`checker.rs` decision.

The falsifier is in the table: if a seeded mean of the commit ever lands
above the base's by more than the seeds' spread, the residual is work and
this note is wrong.

### Ownership and work boundaries (checker port convention)

- **Native operation:** getTypeFromMappedTypeNode's `typeNodeLinks.resolvedType`
  (`checker.go:24170`): one mapped type per node and context.
- **Key identity and owner:** unchanged from r5-mapped5 §1. A built type is
  published in `type_literal_types` under `type_literal_key`. Declined parts
  go on the caller's written-text image through `publish_mapped_type_info`,
  which also enters that image under the parts' `node_key` (`or_insert`, as
  `capture_mapped_type` did).
- **Publication states:** `Built` is published, as before. `Declined` is not
  published, so a later evaluation in another context, or after the
  template becomes evaluable, retries, as before. The change is that one
  evaluation of the node yields both the attempt and the parts.
- **Receiver/alias context:** the parts are evaluated under the current
  alias-evaluation frames, exactly once; the image that receives them is
  minted in the same frames.
- **Expensive work boundary:** `mapped_type_info`'s constraint, template and
  `as`-clause evaluation now runs once per declined node evaluation instead
  of twice. That is 80 × ~40 K Ir on domain-model's `DeepReadonly` bodies.

### What is still eager, and why it is not this lane's to change

The route's remaining real cost is native-shaped:
- `KeysOfType`'s per-key conditional runs in native too, in
  getIndexedAccessType over the instance.
- One fixed cost is not native-shaped: lib's `{ [K in keyof any[]]?: boolean }`
  (`Array[Symbol.unscopables]`, 0.44 M Ir). It resolves and prints all its
  members as soon as inference reads that property. Native never resolves
  them.

Both come from the port computing a type's text at mint (`TypeData::Named`,
`printing::type_to_string(&Type)`), so a non-generic mapped type prints, and
therefore reads, its members when it is created. Deferring the text is
ADR-0050's alternative 1. It needs the printer to compute text on demand,
which is `printing.rs`/`types.rs`, not this lane.

## 2. `.16.71`: a deferred conditional printed from its typed parts (diff, held on 4 losses); `.16.100` not reached

**Forcing constraint.** getTypeFromConditionalTypeNode (`checker.go:24269`)
builds a ConditionalType from typed parts. conditionalTypeToTypeNode
(`nodebuilderimpl.go:2916`) prints it:
- the check type;
- the extends type, with `ctx.inferTypeParameters` set so that an `infer`
  parameter prints as `infer P`;
- getTrueTypeFromConditionalType and getFalseTypeFromConditionalType, the
  written branches instantiated by the conditional's mapper.

emitConditionalType (`printer.go:2058`) emits the check type at
`TypePrecedenceUnion` and the extends type in the extends clause, where a
conditional is parenthesised (`:2275`). It emits the branches at the lowest
precedence. The port's deferred-conditional arm in `declared.rs` mints the
written text instead. That keeps the author's union order, the local alias
names and the `keyof Omit<…>` spelling, where native prints sorted unions,
expanded local aliases and `Exclude<…>`.

**Diff** ([`r5-mapped6-conditional-typed-print.diff`](r5-mapped6-conditional-typed-print.diff),
`declared.rs` (r5-declared3) and `node_reuse.rs` (r5-nodereuse), on top of
§1's route diff):
- `conditional_type_text` prints the deferred conditional from
  `get_type_from_type_node` of its four parts under the current
  alias-evaluation frames, which are the conditional's mapper.
- `node_reuse` gains `binds_below_union` and `binds_below_function`, two
  more rungs of its existing text precedence reader.
- **Divergence kept:** an extends clause that declares an `infer` keeps its
  written text. Native's `infer P` spelling is print context
  (`ctx.inferTypeParameters`) that `type_to_string(&Type)` cannot carry.
- A part that evaluates to `error` keeps the whole written text, as before.

**Measured** on top of §1's commit and route diff, both dumps unfiltered:
- types **+5 RIGHT**:
  - `complicatedIndexesOfIntersectionsAreInferencable:5`
    (`Exclude<keyof …>` for `keyof Omit<…>`);
  - `simplifyingConditionalWithInteriorConditionalIsRelated:17/19` (the local
    alias `One` expanded);
  - `wideningWithTopLevelTypeParameter:47` (`string | T`, sorted);
  - `mappedTypeAsClauses:106`.
- **−4 RIGHT**: `controlFlowGenericTypes:298/300/301/303`;
- diagnostics unchanged; slowcases clean;
- Ir against §1: domain-model 1,159,835,827 → 1,161,583,616 (+0.15%, plain
  run; see §1 on the layout term), generic-imports +0.002%.

**Why it is held.** `type Column<T> = (keyof T extends never ? { id?: number | string } : { id: T }) & …`.
Under `keyof Column<T>` the port evaluates the alias body under the frame
`T := T`. That frame mints a fresh type literal for `{ id?: number | string }`
(`type_literal_key` keys on every binding in scope), and the fresh
literal's members print as types: `string | number`. Native's true branch
is `instantiateType(trueType, mapper)`. getObjectTypeInstantiation
(`checker.go`) keys an anonymous type's instantiation only on the outer
type parameters for which `isTypeParameterPossiblyReferenced` holds, and
with none it answers the type itself. So native prints the original literal,
whose member reuses its written annotation (`number | string`). The
written-text mint hid this. The typed print exposes it.

**What would unblock it.** `type_literal_key` (declared.rs) keys only the
bindings whose parameter the node can reference: a port of
isTypeParameterPossiblyReferenced over the literal's node. That changes
every type literal evaluated under alias frames, so it needs its own
measurement. It is r5-declared3's file.

**`.16.100` (mapped nodes under alias bindings): not reached.** With §1's
route diff, `deeplyNestedMappedTypes` drops from 18 non-RIGHT lines to 13,
and `conditionalTypes2` from 14 to 12. The rest of its eight cases are
untouched.
`deeplyNestedMappedTypes:124–131` prints `PropertiesReduce<…>` where native
resolves the conditional to the mapped object. `mappedArrayTupleIntersections:11`
maps an array-intersection's members where native keeps the tuple.
`declarationAssertionNodeNotReusedWhenTypeNotEquivalent1:16/17` keeps the
generic mapped form of an instance. These are not traced further.

## 3. `mappedTypeNotMistakenlyHomomorphic:0:26`: `keyof` reduces the alias body, not the reference (diff)

**The brief's premise, corrected.** r5-mapped5 §6 blamed an undistributed
`{ v: A } & (X | Y)`. The port does distribute it. `Gen<ABC.A>` is the
union `{ v: ABC.A } & { v: ABC.A; a: string } | { v: ABC.A } & { v: ABC.B; b: string }`.
It prints its denormalized intersection origin, as native's does. A scratch
trace of `is_generic_reducible_type` and `get_reduced_type` showed the
actual order:
- `resolved_keyof_type_worker` calls `get_reduced_type` on the operand. The
  operand is the alias *reference*, which is not a union, so nothing
  reduces.
- `binding_type_alias_body` then unwraps the reference to the union.
- The `reducible_union` test (r5-mapped5's reducible-keyof diff) finds the
  second constituent reducible, because its `v` is `ABC.A & ABC.B`. It
  defers the `keyof`.

Native's getIndexTypeEx reduces the instantiation itself (`checker.go:26685`),
which is the union, so that constituent is gone before shouldDeferIndexType
runs. The keys are `"a" | "v"`.

**Diff** ([`r5-mapped6-keyof-reduced-alias-body.diff`](r5-mapped6-keyof-reduced-alias-body.diff),
`declared.rs` (r5-declared3), on top of §1's route diff):
- `resolved_keyof_type_worker` also reduces the alias body when
  `binding_type_alias_body` unwraps one;
- a new unit test, `tests/keyof_reduced_alias_body.rs`. It fails without
  the diff.

**Measured** against §1 (commit + route diff), both dumps unfiltered:
- types **+1 RIGHT** (`mappedTypeNotMistakenlyHomomorphic:0:26`, now
  `"a" | "v"`; the case is fully RIGHT);
- no other line's text changes; diagnostics unchanged;
- zero losses; slowcases clean;
- Ir: domain-model 1,159,835,827 → 1,160,117,901 (+0.024%), generic-imports
  342,935,705 → 342,914,296 (−0.006%).
