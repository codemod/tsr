# Parity box `type-refs` (tsr-2zk.13) — notes

Judgment calls made by the type-refs box, with the numbers that decided them.
Upstream is `vendor/typescript-go` @ `5b1047d`. Baseline for every number
below: the frozen dumps at `0d996e8` (types 465,643 RIGHT / 1,523 GAP / 9,621
WRONG).

## 1. QUALIFIED port set (tsr-2zk.16.4)

### 1.1 What was ported

`Checker::resolve_entity_name_ex` (`crates/tsr-checker/src/declared.rs`) is
`resolveEntityName` (`checker.go:15772`) with `ignoreErrors = true` and the
alias steps the older `&self` `resolve_entity_name` never had:

- `resolveQualifiedName` (`:15828`) resolves the **left** with meaning
  `Namespace` and `dontResolveAlias = false`, so `import b = a.b`,
  `import * as React` and ES-import lefts are followed before their exports
  are read. Before this, the exports of the unresolved ALIAS symbol (always
  empty) were read, every alias-rooted `X.Y` missed, and the reference fell to
  the ANY-flagged unresolved mint.
- The right name goes through `getExportsOfSymbol` (a module follows `export =`
  with `dontResolveAlias = false`, then `export *`) and `getSymbol`'s meaning
  filter (an alias counts when its chain's flags meet the meaning; unknown =
  all). On a miss over an alias namespace, the exports of
  `resolveAlias(namespace)` are tried (`:15853`).
- The found symbol is walked along its alias chain until it carries the
  meaning (`:15821`).

The `&self` `resolve_entity_name` is kept unchanged: `constraints.rs:628`
(a `&self` fn) and `check.rs:7969` (a lifetime that does not admit `&mut`)
cannot take the `&mut` version. Its other callers would benefit; see the final
report's out-of-scope list.

### 1.2 The written-text mint is kept, except where upstream's type cannot print differently

`qualified_type_reference` still mints the written text for classes,
interfaces, namespace-rooted enums and generics, because routing them through
`getTypeReferenceType` loses the qualifier: the printer cannot yet qualify a
name from its symbol (NB-SYMBOL-CHAIN, ADR-0044 step 2). Measured routing
variants against the baseline, each on top of the alias walk:

| routed through the shared `get_type_reference_type` | R→W | gains |
|---|---:|---:|
| enums (all) | 4 | +4 |
| type aliases (all, argument-less) | 36 | +23 |
| generics (all) | 54 | +8 |
| argument-less classes/interfaces | 174 | +11 |

So three narrower arms were taken instead, each chosen so that no rendered
line can move where upstream's answer would print differently:

1. **Non-generic type alias whose declared type does not carry the alias.**
   Upstream's declared type is the body as built; only alias-accepting
   constructors attach the alias. A pre-existing body (`number`, `undefined`,
   an interface the body merely names) prints as itself, never `N.T`.
   "Carries" is read from this port's print-at-creation representation: an
   alias-attributed union, a type printing as the alias's own name, or a body
   that is directly an array, tuple or type-argument-bearing reference
   (`isDeferredTypeReferenceNode`, `:23236` — `type S = Container<string>` is
   a deferred reference carrying `S`; omitting this rule cost 16 R→W in
   `recursiveGenericUnionType1/2`). Converts `classFunctionMerging`,
   `conditionalTypeRelaxingConstraintAssignability`, `moduleVisibilityTest4`.
2. **Alias-rooted enum whose declared type prints, at the reference site,
   exactly as the mint.** The relater then sees the real enum instead of an
   OBJECT mint (`<foo.E1>0` assigned to `number` reported TS2322 in
   `commonJSImportNotAsPrimaryExpression`). Namespace-rooted enums keep the
   mint: routing them measured 3 R→W through a union's named-constituent guard.
3. **Alias-rooted generic** references take `get_type_reference_type` (arity
   window, defaults). Namespace-rooted generics keep §42 v2's mint (54 R→W).

The alias-rooted scoping of 2 and 3 is a population gate, not an upstream
rule: it confines the new road to references that were unresolved before this
change, leaving every previously measured line on its measured road. It should
disappear when NB-SYMBOL-CHAIN lands, at which point all four variants above
should be re-measured.

### 1.3 Two declines that keep a gap rather than a wrong diagnostic

`alias_rooted_reference_declines` returns an alias-rooted reference to the
unresolved mint it answered before, in two situations where the symbol it now
reaches is answered by machinery this port lacks:

- **Generic heritage.** `interface S<T> extends D<T> { a?: string }` then
  `const y: S<number> = {}` reports a false TS2322 **in the baseline binary
  too, with or without a namespace** (reproduced with `tsr -p` on a two-line
  file). Resolving `React.HTMLAttributes<HTMLElement>` surfaced it in
  `reactTagNameComponentWithPropsNoOOM2` (diagnostic EMPTY_RIGHT→EMPTY_WRONG).
  The bug belongs to the relater / base-type lanes.
- **Module augmentation.** The binder does not run `mergeModuleAugmentation`
  (`checker.go:1407`; `crates/tsr-binder/src/binder.rs` lists it as unported),
  so an augmented module's export table is incomplete. Only augmentations in
  the reference's own file are detected (both witnesses,
  `moduleAugmentationDoes{Interface,Namespace}MergeOfReexport`, have that
  shape); elsewhere this road matches the ES-import road, which ignores
  augmentations too.

Falsifier for each: delete the arm once its owner lands; the named case must
stay EMPTY_RIGHT.

### 1.4 The §605 cycle gate moved before resolution

`circular4` (`export type T = ns2.nested.T` across two files importing each
other) must answer `error`. The gate sat on the *miss* branch; the alias walk
now resolves the name, so the gate runs before the resolution result is used.
Without the move: 2 R→W.

### 1.5 Checker port convention

- **Native operation:** `resolveEntityName` / `resolveQualifiedName` /
  `getSymbol` / `getExportsOfSymbol` @ `5b1047d`; consumer
  `getTypeFromTypeReference` via `qualified_type_reference`.
- **Identity and owner:** no new cache or side table. The existing
  `qualified_reference_types` key `(text, resolved symbol)` is unchanged;
  `resolved` may now be a symbol reached through an alias, which is upstream's
  identity too.
- **Publication:** none added. `resolve_alias` is recomputed per call (as
  before; upstream memoises `aliasTarget`).
- **Work boundary:** per alias-rooted qualified reference, one alias walk for
  the left, one `resolve_alias` for the decline check, and (for a module
  target only) a scan of the reference file's top-level statements. Neither
  bench project contains a namespace import or a qualified annotation, so the
  perf self-ratio cannot reach this code; measured ratios 0.98–1.05 against an
  identical-binary control of 1.02–1.03 at 21 samples.

### 1.6 Result

Types: 465,762 RIGHT / 1,457 GAP / 9,568 WRONG (+119 lines, 0 R→W; +13 cases
fully RIGHT). Diagnostics: 8 WRONG→RIGHT, 0 losses.

Remaining in the set: `externalModuleReferenceDoubleUnderscore1` (`basics`
root identifier and `typeof basics.TimeUnit` print `any`: the types producer's
root-identifier rule, `types_producer.rs`, not owned);
`contextuallyTypedJsxChildren2` (`NoInfer` evaluation); `trackedSymbolsNoCrash`
(`Extract<ast.Node, …>` over the namespace-rooted alias mint — needs arm 1 of
§1.2 for alias bodies that carry the alias, i.e. NB-SYMBOL-CHAIN);
TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL and IMPORT-EQUALS-ALIAS-TYPEREF-TARGET
(the identifier arm's §157/§491 roads) not started.

## 2. Round 2 (baseline frozen at `15f1743`: types 467,200 RIGHT / 1,213 GAP / 9,239 WRONG; diagnostics 3,846 RIGHT / 4,941 EMPTY_RIGHT)

### 2.1 Type-literal index infos are deduplicated by key (`tsr-2zk.16.40`)

`getIndexInfosOfIndexSymbol` (`checker.go:19634`) appends an info for a key
type only when `findIndexInfo` finds none (`:19655`), so two `[x: number]`
signatures in one literal yield ONE info and print one row.
`build_type_literal` (`declared.rs`) now skips a later index declaration whose
key type it already holds. `index_signature_member` declines union keys before
this point, so the key is a single type and identity is the right test.
Measured: +6 lines, 4 cases (`duplicateNumericIndexers`,
`duplicateStringIndexers`, `multipleNumericIndexers`, `multipleStringIndexers`),
0 losses in either dump. No new cache or side table.

### 2.2 First ADR-0045 slice: alias-free generic bodies declare their body

ADR-0045 rule 4 (the declared type is the body as built, the alias lands only
through an alias-accepting constructor) applied to the three body kinds whose
upstream constructors never receive `getAliasForTypeNode`: a template literal
(`getTypeFromTemplateTypeNode`), `keyof X` (`getTypeFromTypeOperatorNode`) and
a type query. `get_declared_type_of_type_alias` answers the resolved body for
those instead of the `Name<Params>` mint; a body this port cannot resolve
(`typeof g<V>`, an instantiation expression) keeps the mint. Measured: +27
lines, 1 case (`templateLiteralTypes8`), 0 losses. References are unchanged:
`instantiate_template_alias` already evaluated template bodies, and `keyof`
references keep their `create_type_reference` road (`K<{ a: 1 }>` still
prints the reference where upstream prints `"a"` — the instantiation half,
rule 3, is not built).

No new table: this changes the value stored in the existing `declared_types`
owner, under the existing `resolutions` frame.

### 2.3 Held patch A, measured, blocked on two annotation-reuse sites outside this lane

Patch: `type-refs-held/A-dup-members-and-type-parameter-body.patch` (applies to
this branch's head; NOT built into the tree — it touches `signatures.rs` and
`objects.rs`, which this box does not own).

Two more slices are built and measured but NOT committed, because each moves a
signature print that upstream produces by reusing the written annotation
(`serializeTypeForDeclaration`), at a site this box does not own:

- **Duplicate property signatures merge** (`tsr-2zk.16.54`): `declareSymbolEx`
  merges same-named property signatures, so `{ a: string; a: string; }`
  prints `{ a: string; }` everywhere except a reused annotation.
  `checkTypePredicateForRedundantProperties` (a passing case) wants the
  written `x is { a: string; a: string; }`. Needs `signatures.rs`
  `type_predicate_from_node` to read `qualified_written_text` for ANY
  annotation node, not only `TypeReferenceNode`.
- **Type-parameter body instantiation** (`instantiateTypeWithAlias` over a
  type parameter returns the image): `type Id<T> = T; type X = Id<string>`
  is `string`. `divergentAccessorsTypes6` (passing) wants
  `set x(value: Fail<string>)`. Needs `objects.rs`'s divergent-setter print
  to use `written_annotation_text(annotation)` before `type_to_string`.

With both one-line changes applied (measured together with 2.1/2.2's
predecessor): 18 cases, +43 lines, 0 losses — the held slices add
`conditionalTypeAnyUnion`, `divergentAccessorsTypes6`, `propertySignatures`,
`duplicatePropertiesInTypeAssertions01/02`, `duplicatePropertyNames`,
`numericNamedPropertyDuplicates`, `stringNamedPropertyDuplicates`,
`objectTypeWithDuplicateNumericProperty`, `unknownType2`,
`intersectionApparentTypeCaching`, `inferTypeParameterConstraints`,
`relatedViaDiscriminatedTypeNoError2`, `importClause_namespaceImport`.

### 2.4 Held patch B: deferred type references carry their alias (ADR-0045's first table)

Patch: `type-refs-held/B-deferred-type-reference-alias.patch` — NOT built into
the tree. It is the first writer and reader of ADR-0045's side table:

- `Checker::alias_of: TypeId -> (alias SymbolId, alias arguments)` and the
  intern table `deferred_alias_references: (alias, reference) -> TypeId`
  (checker.rs fields; ADR-0045 rule 1's representation).
- Writer: `deferred_alias_reference` (declared.rs) — `createDeferredTypeReference`
  through `isDeferredTypeReferenceNode`'s alias arm (`checker.go:23236`), for an
  array node (`getTypeFromArrayOrTupleTypeNode :24115`) or a generic
  class/interface reference (`getTypeFromClassOrInterfaceReference :23200`) that
  is directly the body of a NON-generic alias. The copy keeps flags, member
  owner and `type_reference_targets`, so every semantic consumer sees the same
  `(target, arguments)`; only the printer reads `alias_of`.
- Reader: `type_to_string_at` prints `alias_of` first through
  `reference_text_at` (ADR-0045 rule 5, the node builder's alias arm `:3362`).
- `without_alias` recovers the canonical reference for consumers that build a
  NEW type from the structure: the variadic-tuple rest print
  (`createNormalizedTupleType` stores a rest's element type; `[...Numbers, boolean]`
  prints `[...number[], boolean]` — measured 3 R→W without it).

Checker port convention: native operation `createDeferredTypeReference` /
`isDeferredTypeReferenceNode` @ `5b1047d`; key `(alias SymbolId, canonical
reference TypeId)`, owner the Checker; published once at creation, never
mutated, absent = no alias (complete); consumer context printing only; work
boundary one hash lookup per alias-body array/class reference, which runs once
per alias because the declared type is memoised.

Measured (on top of 2.1/2.2): +158 type lines, 11 cases
(`constraintOfRecursivelyMappedTypeWithConditionalIsResolvable`,
`genericDefaultsErrors`, `instanceofTypeAliasToGenericClass`,
`selfReferencingTypeReferenceInference`,
`typeVariableConstraintedToAliasNotAssignableToUnion`,
`destructuringParameterDeclaration3ES5/ES6`, `destructuringParameterDeclaration4`,
`directDependenceBetweenTypeAliases`,
`objectTypeWithStringAndNumberIndexSignatureToAny`, `readonlyArraysAndTuples2`).
Losses at round 2, which is why it was held (both now resolved — §3.1):

1. `recursiveTypeReferences1` (2 lines, `children.length` → `any`):
   `members.rs` `completed_array_placeholder_length_body` requires
   `instantiations[key] == declared_types[owner]`, which the alias copy is not.
   Fixed by `self.without_alias(...)` on the declared body.
2. `spreadBooleanRespectsFreshness` (1 line): `c ? fa : [fb]` with
   `fa: FooArray` printed `FooArray`; upstream prints `FooBase[]`. Diagnosed in
   round 3 (§3.1): a union-sort defect, not the relater.

### 2.5 Generic aliases run the declared-type push/pop frame (TS2456)

Requested by the integrator ahead of the parser box's type-parameter-list
recovery, which turns `type T1<in in> = T1` into a GENERIC alias
(`varianceAnnotationsWithCircularlyReferencesError`: upstream `>T1 : any` plus
TS2456). Two upstream orderings were missing:

- `getDeclaredTypeOfTypeAlias` (`checker.go:23837`) resolves the body under
  `pushTypeResolution` for every alias, generic or not. The generic arm of
  `get_declared_type_of_type_alias` now does the same before its `Name<Params>`
  mint: a failed pop reports TS2456 and declares `errorType`; a mention from a
  lazily resolved construct (`deferred_since`) answers the mint, the §29 seam.
  The resolved body is discarded (the declared type is still the mint — ADR-0045
  rule 4 is not built for these bodies).
- `getTypeFromTypeAliasReference` (`:23580`) reads the declared type BEFORE the
  arity window, and a circular alias publishes no type parameters, so the
  self-reference is `errorType`, not a TS2314. `get_type_reference_type` now
  asks for a generic alias's declared type first and answers `error` on error.
- `check_type_alias_circularity` (check.rs, the type-alias check) no longer
  skips generic aliases, so the check pass reaches the getter.

Measured on this branch (no parser change): zero moved lines in either dump
against §2.2's state — no existing generic alias is newly found circular. On a
scratch merge with the parser branch (`0238e8f`), the case's types turn
2/2 RIGHT and its two TS2456 diagnostics appear (the remaining misses are
TS2637 and TS1359, not this item).
- Conditional-type BRANCHES are now deferred boundaries in
  `native_resolves_lazily`. The first build of this commit failed
  `awaited_types.rs`: lib `Awaited<T>` recurses through `Awaited<V>` in a
  branch, which upstream never resolves while the declared type resolves
  (`getConditionalType`, `:24300`, reads the branches only once the
  conditional is not deferred) but which this port resolved eagerly — a false
  circularity, `x: Awaited<T>` printing `error`. The port cannot yet tell at
  that point whether a conditional defers, so every branch is a boundary;
  for a fully concrete conditional that is lazier than upstream, and the cost
  is a missed TS2456, never a false one. Falsifier: a concrete conditional
  alias whose branch names itself, where upstream reports TS2456 and this port
  does not.

Re-measured with the branch boundary: zero moved lines in either dump,
workspace tests pass, and the scratch-merge result above still holds.

## 3. Round 3 (baseline frozen at `d109b0c`: types 467,948 RIGHT / 1,175 GAP / 8,794 WRONG; diagnostics 4,010 RIGHT / 4,957 EMPTY_RIGHT)

### 3.1 Held patch B's last loss is `CompareTypes`, not the relater

`spreadBooleanRespectsFreshness` line 10 is the conditional
`Array.isArray(foo2) ? foo2 : [foo2]`, whose type is
`getUnionTypeEx([FooArray, FooBase[]], UnionReductionSubtype)`. The two
constituents are mutually strict subtypes (same `Array` target, same
argument), so `removeSubtypes` (`checker.go:25934`) removes whichever sorts
LAST — it walks the sorted list from the end and deletes the first source it
finds related to another member. The order is `CompareTypes`
(`utilities.go:415`), whose second key is `compareTypeNames` (`:589`), and
`getTypeNameSymbol` (`:607`) answers **the alias symbol first**: the deferred
reference carrying `FooArray` sorts under "FooArray", the fresh array literal
under its target's "Array". "Array" < "FooArray", so upstream's list is
`[FooBase[], FooArray]`, `FooArray` is removed, and the line prints
`FooBase[]`.

This port's `compare_type_names` (`unions.rs`) saw two entries of
`type_reference_targets` with the same target, compared their argument lists
(equal), and fell through to the type-id tiebreak — creation order, which put
the alias copy (created when `Foo` resolved) first. The fix reads
`alias_of` before the reference arm: two types carrying the same alias
compare by alias arguments (`:593`); otherwise an aliased side is named by
its alias. It is upstream's rule, not a tiebreak chosen to make the case
pass: the falsifier stated in round 2 ("line 10 turns RIGHT and no other
line moves") held — the full dumps move only gains.

`unions.rs` belongs to the contextual box this round, so patch B stays held,
now including that hunk, the `members.rs` `without_alias` wrap this box owns,
and the matching update of that function's unit test (it asserted the
declared type, which is now the alias copy; the function answers the
canonical reference).

Measured against this round's baseline (patch applied to `d109b0c`): **+161
type lines, 12 cases fully RIGHT** (round 2's 11 plus
`spreadBooleanRespectsFreshness`), **0 R→W in either dump**, diagnostics
unchanged (4,010 RIGHT / 4,957 EMPTY_RIGHT). Perf, median child CPU over 21
samples against the baseline binary: domain-model 1.019, generic-imports
1.023 (both ≤ 1.03; `diagnostics_match: true`). Workspace tests pass with the
patch.

### 3.2 A stale unit test

`tests/types.rs` `a_reference_to_a_generic_type_carries_its_arguments`
asserted `type A<T> = T; declare const x: A<number>` prints `A<number>`. The
tree at `d109b0c` already prints `number`, which is upstream's answer
(`instantiateTypeWithAlias`, `checker.go:22104`, returns a bare type
parameter's image), so the test failed at the baseline and its expectation
is flipped.

### 3.3 `trySymbolTable`'s ExportSymbol candidate replaces the import-equals exclusion (`tsr-2zk.39`, `tsr-2zk.16.20`)

`best_name` (checker.rs) is this port's whole-name and segment-name walk
over `getAccessibleSymbolChain`'s scope tables. Two deviations from
`trySymbolTable` (`symbolaccessibility.go:535`) compensated for each other:

- A table entry whose **ExportSymbol** is the target answered the symbol's
  own name immediately. Upstream (`:551`) only appends `[symbol]` to the
  candidate chains, which then sort with the alias candidates by
  `compareSymbolChains` (length, then `compareSymbols`: first declaration's
  file and position).
- Because the export hit never competed, admitting a same-file
  `import a = B` alias let it win in tables where upstream's exported
  declaration wins, so the alias was excluded from whole-name prints (`admit_local_import_equals = false`) after admitting it lost 130
  `privacy*` lines (`checker-notes-modobj.md` §10.16–17).

Ported: the ExportSymbol hit seeds the alias competition as candidate
`(own name, symbol)`; the exclusion and the parameter are deleted, so every
caller runs the same walk. In the `privacy*` shape the exported namespace is
declared before the alias, so its own name still wins; in
`namespace B { import O = Outer; … }` (`constEnumOnlyModuleMerging`) the
table holds the alias and no local of `Outer`, so `O` is printed, as
upstream does.

Measured against `d109b0c` (without patch B): **+94 type lines, 11 cases
fully RIGHT** (`circularReferenceInImport`, `constEnumOnlyModuleMerging`,
`importInTypePosition`, `importedModuleAddToGlobal`, `innerAliases2`,
`moduleAliasInterface`, `moduleCrashBug1`, `moduleVisibilityTest3`,
`unusedImports10`, `importStatements`, `tsxElementResolution7`), **0 R→W in
either dump**.

Checker port convention: no cache, side table or traversal is added; the
same per-print scope walk runs with one more candidate per table. Work
boundary unchanged (one `compare_symbols` per ExportSymbol hit).

Not ported here (remaining `tsr-2zk.39`): `isAccessible`'s
`canQualifySymbol` test on the direct and ExportSymbol arms (the walk
assumes every hit is qualifiable), `getCandidateListForSymbol` recursion
deeper than one export level, and `getWithAlternativeContainers`
(`SYMBOL-CHAIN-EXPORT-EQUALS-CONTAINER`). `own_name_alias_at` still
requires `flags == ALIAS` exactly (`shadowedInternalModule`'s merged
alias+var), which belongs to the same walk.

### 3.4 A merged alias still names its target (`tsr-2zk.16.20`)

`own_name_alias_at` stops qualification when the bare name resolves at the
site to an alias of the target. It required the hit's flags to be exactly
`ALIAS`. Upstream's binder excludes only `Alias` from an alias declaration
(`AliasExcludes`), so `import Y = X.Y; var Y = 12` is one symbol carrying
`Alias | FunctionScopedVariable` (the conflict is a checker diagnostic, not a
binder split), and `trySymbolTable` iterates it as an alias (`:562`). The
test is now `contains(ALIAS)`; the immediate-target comparison that guards
merged namespace/alias symbols is unchanged.

Measured against the merged baseline (`246056c`: types 468,044 RIGHT,
diagnostics 4,050 RIGHT / 4,958 EMPTY_RIGHT): +4 type lines, 1 case
(`moduleSharesNameWithImportDeclarationInsideIt4`; `shadowedInternalModule`
line 26 turns RIGHT, the case has other gaps), 0 R→W in either dump. No new
state.

### 3.5 A type-literal method named `new` prints quoted (contextual lane's ask)

`classifyPropertyName` (`nodebuilderimpl.go:2384`) answers a string literal
for a METHOD named `new`, so it cannot read back as a construct signature:
`var c: { new?(): any }` records `{ "new"?(): any; }`. `build_type_literal`
(declared.rs) carries the printed name on the member tuple only (the same
channel as the optional `?`), so the quote changes printing and nothing
else. Only the type-literal method half is here; object-literal and class
methods are printed by other lanes' code. +2 lines, 2 cases (`vardecl`,
`parser645484`), 0 R→W.

### 3.6 `keyof` alias references instantiate their body (ADR-0045 rule 3, one body kind)

**Held patch C** (`type-refs-held/C-keyof-alias-instantiation.patch`). This change
is NOT in the tree. Besides the owned `templates.rs` change, it needs two hunks
outside this box's files:

- `signatures.rs` type-parameter `written_constraint`: tsgo's
  `typeParameterToDeclaration` (`nodebuilderimpl.go:1615`) reuses the written
  constraint node, so `<K extends Key<T>>` prints `Key<T>` even though the
  type is now `keyof T`. Without this hunk the probe
  `computed_indexes::computed_keyof_alias_constraint` (pinned tsgo) fails.
- `tsr-conformance/tests/original_callable_entry.rs`: the test asserted a
  TS2464 that is now gone. Native's bag is empty, so the expectation becomes
  `[]`.

With all three hunks applied: workspace tests pass and fmt is clean. Perf
(median CPU, 41 samples): domain-model 0.976, generic-imports 0.991.

§2.2 made a `keyof X` alias body its own declared type but left references on
the `create_type_reference` road, so `type KeyOf<T> = keyof T` printed
`KeyOf<{ a: 1 }>` where upstream prints `"a"`. Upstream's
`getTypeFromTypeOperatorNode` takes no alias, so the declared type is an
index type and `getTypeAliasInstantiation` → `instantiateType` re-runs
`getIndexType` on the instantiated operand. `instantiate_template_alias`
(templates.rs) already evaluated a template body under the alias's
argument bindings; it now admits a `keyof` operator body on the same road
(same in-progress guard, same `alias_evaluation_bindings` frame, result
cached in `instantiations` by the caller). A generic argument yields
`keyof U`, which is also upstream's print.

Measured against `3612144` (patch applied): +9 type lines (`keyofIntersection`,
`mappedTypeAsClauses` 6, `recursiveTypeRelations` 2), diagnostics
`mappedTypeAsClauses` WRONG→RIGHT and
`declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`
EMPTY_WRONG→EMPTY_RIGHT, 0 losses in either dump. No new state: the
instantiation cache entry is the existing `instantiations[(alias, args)]`.
