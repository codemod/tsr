# Lane notes: r6-errorsplit3 (tsr-2zk.1150, continuing r6-errorsplit2)

Third box on the intrinsic/error contract. The decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md);
the previous step is [r6-errorsplit2](r6-errorsplit2.md). Pinned upstream:
`vendor/typescript-go` @ `5b1047d`. Owned files: `indexed.rs`, `spreads.rs`,
`jsx_intrinsic.rs`, `index_signatures.rs`, `import_meta.rs`,
`assignment_declarations.rs`, `readonly_target.rs`, `types_producer.rs`.

The session was cut short by the round's usage checkpoint. What was measured
is marked with its numbers; everything else is marked **UNMEASURED**.

## §1 Base and instrument

**Base.** Batch BT (r6-errorsplit2) landed during the session. The base is
frozen at the integration tip `80bd4ba`, unfiltered:

- types 550,423 RIGHT / 752 GAP / 5,128 WRONG (556,303 aligned lines);
- diagnostics 5,643 RIGHT / 5,607 EMPTY_RIGHT / 950 WRONG / 38 EMPTY_WRONG;
- probe join: 1,486 credited gap lines (1,420 `errorType`, 66 `anyType`
  natively); 31,851 matched `native_error` lines, all `errorType` natively.

Before BT landed, work ran on `06797e5` with BT's eight diffs applied (the
emulation reproduced r6-errorsplit2 §15). The diffs below measured there
first; diff G was re-measured on `80bd4ba`.

**The probe** is r6-errorsplit2 §1's: `go test -c -overlay` over
`type_symbol_baseline.go` and `compiler_runner.go`, tagging `@@E`, `@@A` and
`@@X`. The port side is a measurement-only example (never committed) that
dumps every gap, `native_error` or rewritten line with its file, rewrite,
producer and expected text. A join on `(case, file, position)` requires the
probe's text to equal the expected text. `PyPI` is blocked: `assemble.py`'s
`tomlkit` calls ran against a stdlib stand-in outside the repository.

## §2 Item 1: the read-reference property miss, bucketed

r6-errorsplit2 §3 gated `checkPropertyAccessExpressionOrQualifiedName`'s
`prop == nil` exit (`checker.go:11353-11369`) to receivers that are no flow
reference. Behind a temporary switch that drops the gate (reads and writes),
on BT's content:

- **+427** `errorType` lines;
- **46 false claims**: 41 untagged (not any natively), 5 `anyType`.

Every false claim is the port's receiver being wrong before the lookup. By
cause (function, file, owner), with the held diff that removes it:

| cause | lines | cases | fix | file (owner) |
|---|---:|---|---|---|
| `typeof globalThis` has no members off its exact-receiver arm | 12 | `truthinessCallExpressionCoercion2` | G | `members.rs` (main) |
| a global augmentation of a UMD alias does not reach the module | 8 | `exportAsNamespace_augment` | U | binder `lib.rs` (main) |
| a reserved word after a dot in a namespace name | 6 | `ambientModuleDeclarationWithReservedIdentifierInDottedPath{,2}` | D | parser `module.rs` (main) |
| `instanceof` only narrows by class callees | 3 | `inKeywordAndIntersection` 2, `typeGuardsWithInstanceOfByConstructorSignature` | I | `flow.rs` (main) |
| `isReadonlySymbol` misses property signatures | 2 | `controlFlowAliasing` | R | `flow.rs` (main) |
| `undefined` vs a template literal is undecided | 1 | `discriminatedUnionTypes3` | T | `relater.rs` (r6-relater3) |
| a never-reduced intersection blocks the discriminant | 2 | `discriminatedUnionTypes2` | N | `flow.rs` (main) |
| `NoInfer` is not stripped or seen through | 1 | `narrowingNoInfer1` | NI | `constraints.rs`, `members.rs`, `flow.rs` (main) |
| a class extending `any` has no `string` index | 2 | `extendFromAny` | **landed** (§3) | `index_signatures.rs` |
| incomplete loop types (`getTypeAtFlowLoopLabel`) | 3 | `controlFlowWithIncompleteTypes` | — | `flow.rs` (main) |
| an instantiated mapped predicate is a memberless mint | 2 | `typeGuardNarrowBy{,Mutable}UntypedField` | — | `mapped.rs` (r6-declared2) |
| a conditional type's union constraint (`Extract`) | 2 | `deeplyNestedConstraints`, `controlFlowGenericTypes` | — | `constraints.rs` (main) |
| a destructuring default's type | 1 | `nonPrimitiveAndEmptyObject` | — | `destructure.rs` (main) |
| `typeof ns.A` of a type-only export | 1 | `namespaceImportTypeQuery` | — | `symbols.rs` (main) |

37 of the 46 have a fix (35 held, 2 landed). The 9 without one are the
REMAINS of item 1. The gate stays until the stack lands and the switch is
re-measured on it (it was to be the stack's last step, `X1ON`, and was not
reached).

Each fix, with its native anchor:

- **G** `resolveAnonymousTypeMembers`' `globalThisSymbol` arm
  (`checker.go:20674-20682`): `typeof globalThis` keeps the globals that are
  neither block-scoped nor an ambient module, and `getPropertyOfObjectType`
  returns one only when it is a value. The port answered `globalThis.x`
  through a receiver arm in `access_member_lookup`; a composite receiver
  (`Window & typeof globalThis`) reached `get_property_of_type_ex`, where the
  symbol-less mint returned nothing. The diff reads the globals there.
- **U** `mergeSymbol` resolves a non-transient alias target
  (`checker.go:14153-14164`); the UMD alias resolves to its file's module
  (`getTargetOfNamespaceExportDeclaration`), the source merges into it, and
  `mergeSymbolTable` stores the merged module in `globals` in the alias's
  place. The binder declined every alias-target merge. The diff does this
  one, in `merge_module_augmentations` before the module augmentations, as
  `initializeChecker` orders them (`:1335-1349`). A pair whose excludes hit
  the module stays declined for `report_merge_conflicts`.
- **D** `parseModuleOrNamespaceDeclaration`'s `nested` arm (`parser.go:2208`):
  a segment after a dot is `parseIdentifierName`.
- **I** `narrowTypeByInstanceof` over `getInstanceType` (`flow.go:964-976`):
  the port took a class-only shortcut for every anonymous callee and read
  construct signatures only off named types. The diff takes the class road
  only for class symbols, and reads signatures through
  `signatures_of_type_kind` (unions, intersections, type literals).
- **R** `isReadonlySymbol` (`checker.go:13857`) reads
  `getDeclarationModifierFlagsFromSymbol`: a property signature or parameter
  property with `readonly`, not only a class property. That makes
  `outer.obj` a constant reference, so an aliased condition narrows it.
- **T** A template literal target relates only what
  `isTypeMatchedByTemplateLiteralType` matches. A non-string primitive
  source (and `undefined`/`null` under `strictNullChecks`) answered
  `Unknown`, which made `narrowTypeByDiscriminant` decline. Relater-lane
  file: shipped as a diff for r6-relater3.
- **N** `getPropertyOfType` reads the reduced apparent type: an intersection
  whose discriminants conflict (`a & b` from `(a | b | c) & (b | c)`) reduces
  to `never`, which `createUnionOrIntersectionProperty` skips. The port's
  discriminant union property declined on it.
- **NI** `getNarrowableTypeForReference` strips `NoInfer`
  (`checker.go:31492`); `getApparentType` reads through the substitution's
  base. The port mints `NoInfer<T>` as an alias reference
  (`no_infer_base_type`); the diff reads through it at those two sites and
  at `getTypeOfPropertyOrIndexSignatureOfType`.

**Measured** (unfiltered, both dumps):

- G on `80bd4ba` + §3: **zero losses; types +36 WRONG→RIGHT, diagnostics
  +1** (`truthinessCallExpressionCoercion2` 15 and its diagnostics,
  `uncalledFunctionChecksInConditional2` 5, `inKeywordTypeguard` 8,
  `arrowFunctionContexts` 4, `multiExtendsSplitInterfaces1` 2,
  `globalThisAmbientModules`, `globalThisBlockscopedProperties`). Same
  numbers on the emulated base.
- U and D together on G, emulated base (`06797e5` + BT): **zero losses;
  types +22 WRONG→RIGHT** (`exportAsNamespace_augment` 11, the two
  dotted-path cases 10, `sourceFileMergeWithFunction` 1), **diagnostics +4
  and +1 EMPTY_WRONG→EMPTY_RIGHT** (`exportAsNamespace_augment`, the two
  dotted-path cases, `noCrashUMDMergedWithGlobalValue`). One GAP→WRONG:
  `noCrashUMDMergedWithGlobalValue`'s `value : SomeInterface` prints
  `import("./other")`, because an interface merged into a module symbol
  prints by the module's name; native names it by its globals key. That is
  the printer's (`printing.rs`, r6-printer4), not a loss.
- I, R, T, N, NI: **UNMEASURED** corpus-wide. Each was checked on its cases
  with the probe file pipeline (every line of `inKeywordAndIntersection`,
  `typeGuardsWithInstanceOfByConstructorSignature`, `discriminatedUnionTypes2/3`
  and `narrowingNoInfer1` matches with the gate dropped; `controlFlowAliasing`'s
  f26 lines match). Each carries a test.

## §3 Item 2: `anyBaseTypeIndexInfo` (landed)

`resolveObjectTypeMembers`' base loop (`checker.go:19143-19150`) gives a base
type that is exactly `anyType` the `anyBaseTypeIndexInfo` (`string` keys,
`anyType` values, `:1048`) unless an own index claims `string`.
`resolveAnonymousTypeMembers` (`:20683-20702`) gives it to the static side of
a class whose base constructor type is exactly `anyType`, when the class
declares no static index signature. The port's index walk failed to follow
such a base and answered the gap.

The commit ports both arms in `index_infos_of_symbol_worker`. Identity with
`intrinsics.any` matters: an `errorType` base (an unresolved `extends`) gets
nothing, as natively (the test pins both). Checker port boundary: no new
cache; the info joins the existing `symbol_index_infos` publication, keyed
`(owner, static)`. The added work is one `get_base_types` (memoized) per
instance walk of a class.

**Measured** on `80bd4ba`: zero transitions on both dumps. 11 lines leave the
gap and print `any` by computing it: `extendFromAny` 8, `classExtendingAny` 3,
all `anyType` natively. Test: `tsr-conformance/tests/any_base_type_index_info.rs`.

The other two `anyType` producers of item 2 were not reached:

- binding elements with neither annotation nor initializer
  (`fallbackToBindingPatternForTypeInference` 10, `declarationsAndAssignments` 4,
  `destructuringArrayBindingPatternAndAssignment2` 5, `iterableArrayPattern21` 2,
  `bindingPatternCannotBeOnlyInferenceSource` 2, `mappedTypeConstraints2` 3,
  `destructuringTuple` 1). These are several producers, not one: the implied
  type of a binding pattern (`getTypeFromBindingElement`, contextual
  parameter types), a non-iterable's `anyType` element
  (`checkIteratedTypeOrElementType`), and an array literal padded by a
  binding pattern. `destructure.rs` is main's.
- `yield` results (`crashInYieldStarInAsyncFunction`,
  `genericCallAtYieldExpressionInGenericCall1`, `types.asyncGenerators.es2018.2`).

## §4 Item 3: the no-value aliases, native's rule

At the base, `getTypeOfAlias`'s "no value declaration" producer holds 192
RIGHT lines: 180 `errorType`, 12 `anyType`. Native's rule
(`checker.go:18598-18627`) is one test: a value target answers
`getTypeOfSymbol(target)`, anything else `errorType`. The two identities come
from what the target *is*:

- **`errorType`**: the target is `unknownSymbol`, whose type is `errorType`.
  `resolveAlias` gives `unknownSymbol` for an unfindable or untyped module
  (`untypedModuleImport`, `packageJsonMain`, `moduleResolution_*`), a missing
  export (`namedImportNonExistentName`, `importNonExportedMember*`,
  `reexportedMissingAlias`), and an alias chain that meets its own
  `resolvingSymbol` mark (`circular1/3`,
  `recursiveExportAssignmentAndFindAliasedType1-6`).
- **`anyType`**: the target is a value whose type is `anyType`. Either a
  shorthand ambient module reached through a specifier, since
  `getExternalModuleMember` returns the module itself
  (`checker.go:14692`; `ambientShorthand_reExport`,
  `importExportInternalComments`' re-exports, `esModuleInteropTslibHelpers`'
  `export { Bar }`), or a variable inside a type circularity
  (`recursiveExportAssignmentAndFindAliasedType7`'s `var selfVar = self`,
  `selfReferentialDefaultNoStackOverflow`): `getTypeOfVariable` reports the
  circularity and answers `anyType`.

The port's gap is `resolve_alias`' `None`, which mixes `unknownSymbol` with
chains the port cannot follow (the 50 OTHER lines under the same producer
are real values natively). So the diff ports the two arms that decide the
identity, not a wholesale switch:

- **S** (`symbols.rs`, `checker.rs`; main's):
  - `getExternalModuleMember`'s shorthand arm: a named import or re-export
    from `declare module "m";` resolves to the module, typed `anyType` by the
    existing `getTypeOfFuncClassEnumModule` arm.
  - `resolveAlias`' circular arm: a resolution stack records every alias
    between a re-entered mark and the top, and `get_type_of_alias` answers
    `native_error` for those. Checker port boundary: the stack
    (`alias_resolution_stack`) is the native `resolvingSymbol` marks, owned
    by `resolve_alias`, live only while workers run; the set
    (`circular_aliases`) is published once per alias, never cleared, keyed
    by the alias symbol as passed. No added work beyond a push and pop per
    worker.

**UNMEASURED** corpus-wide (it compiles and its test is written). The
type-circularity `anyType` lines stay the gap.

## §5 Item 4: diff A's limit (held)

r6-errorsplit2's diff A gave `foo[k] = v` every same-named late declaration
only when the receiver was an identifier. Native's binder files the
assignment under `lookupSymbolForPropertyAccess`'s symbol, so `a.b[k] = v` is
filed under `a`'s export `b`. Diff **AD** walks an entity-name receiver the
same way (identifier by name, `a.b` as an export of `a`'s symbol) and keeps
the existing `__assignment` containment check as the guard, so a symbol the
walk reaches but the binder did not use answers nothing. It is in my file,
but the session ended before it was measured or its test was run, so it is
held as a diff with the test `tsr-checker/tests/late_bound_dotted_receiver.rs`.
**UNMEASURED.**

## §6 Item 5 (`AtLocation`, `StatementName`): not reached

r6-errorsplit2 §14 found both mixed. Nothing new was measured.

## §7 Apply order

Each diff applies on `80bd4ba` plus this branch's commit, and the ten apply in
this order and compile with their tests (checked):

1. G `r6-errorsplit3-global-this-composite-members.diff` (`members.rs`) — measured, +36/+1;
2. U `r6-errorsplit3-umd-alias-augmentation.diff` (binder `lib.rs`) — measured with D;
3. D `r6-errorsplit3-dotted-namespace-name.diff` (parser) — U+D +22/+5;
4. I `r6-errorsplit3-instanceof-general-road.diff` (`flow.rs`) — UNMEASURED;
5. R `r6-errorsplit3-readonly-property-signature.diff` (`flow.rs`) — UNMEASURED;
6. T `r6-errorsplit3-template-literal-relation.diff` (`relater.rs`, r6-relater3's) — UNMEASURED;
7. N `r6-errorsplit3-never-discriminant-constituent.diff` (`flow.rs`) — UNMEASURED;
8. NI `r6-errorsplit3-no-infer-reference.diff` (`constraints.rs`, `members.rs`, `flow.rs`) — UNMEASURED;
9. S `r6-errorsplit3-alias-target-identity.diff` (`symbols.rs`, `checker.rs`) — UNMEASURED;
10. AD `r6-errorsplit3-late-bound-dotted-receiver.diff` (`assignment_declarations.rs`) — UNMEASURED.

Then the read-reference switch: drop `access_receiver_is_flow_reference` from
`property_access_receiver_is_complete` (`indexed.rs`) and re-measure with the
probe. Expected from §2: 9 false claims left, so it still cannot land whole.

## §8 What remains, with causes

- Item 1's 9 unfixed false claims (§2 table, rows with no fix): incomplete
  loop types, the memberless mapped mint, conditional-type constraints, a
  destructuring default, a type-only `typeof ns.A`.
- Item 2: the binding-element and `yield` producers (§3).
- Item 3: S unmeasured; the type-circularity `anyType` aliases.
- Items 4 and 5: AD unmeasured; `AtLocation`/`StatementName` unreached.
- The perf gate (Ir on domain-model and generic-imports) was not run for the
  commit; its added work is one memoized `get_base_types` per class index walk.
