# r5-declemit2 — declaration-emit diagnostics, round 5

Lane notes for round-5 box `r5-declemit2` (`bd tsr-2zk.969`), continuing
`r4-declemit` (`docs/parity/notes/r4-declemit.md`, `bd tsr-2zk.904`). Pinned
upstream: `vendor/typescript-go` @ `5b1047d`. Same owned files as r4-declemit:
`crates/tsr-dts/`, `crates/tsr-checker/src/symbol_access.rs`, and the
`GetDeclarationDiagnostics` block (with its `EmitResolverAdapter`) of
`crates/tsr-conformance/src/diagnostics_suite.rs`.

## 1. The population, measured

Box baseline: integration head `ac56208` (`origin/main`, which fast-forwards
`405b55ce` by one contextual commit), `diagverdictdump` over the plain and
configured keys (ADR-0047 keys carry `(`). Every judged case whose mismatch
involves TS2883, TS4xxx or TS9xxx, by producer upstream:

| Producer upstream | Codes | Cases |
|---|---|---|
| node builder `ReportLikelyUnsafeImportRequiredError` (`nodebuilderimpl.go:709`) | 2883 | 12 — 6 plain, plus `nodeModulesExportsBlocksSpecifierResolution` and `nodeModulesExportsSourceTs` × 4 `module` variants |
| node builder `TrackSymbol` → `IsSymbolAccessible` | 4023, 4025, 4032 | 6 — `declarationEmitComputedPropertyNameSymbol1/2`, `declarationEmitReadonlyComputedProperty`, `jsDeclarationsTypeReassignmentFromDeclaration2` (JS), `globalThisDeclarationEmit`, `declarationEmitExpandoPropertyPrivateName` |
| node builder `ReportPrivateInBaseOfClassExpression` (`:2665`) | 4094 | 2 — `privateFieldsInClassExpressionDeclaration`, `declarationEmitMixinPrivateProtected` |
| node builder `ReportInaccessibleThisError` / `…UniqueSymbolError` (`:3356`, `:3322`) | 2527 | 2 — `declarationFiles`, `declarationEmitExpressionWithNonlocalPrivateUniqueSymbol` |
| node builder `ReportNonSerializableProperty` (`:2510`) | 4118 | 1 — `declarationEmitMappedTypeTemplateTypeofSymbol` |
| `transformImportDeclaration`'s augmentation arm (`transform.go:2562`) | 9026 | 1 — **converted, §2** |
| **checker**, not declaration emit | 4105, 4109/4110, 4111, 4113/4114 | 6 — `indexedAccessPrivateMemberOfGenericConstraint`, `circularTypeArgumentsLocalAndOuterNoCrash1`, `circularlyReferentialInterfaceAccessNoCrash`, `noPropertyAccessFromIndexSignature1`, `override21`, `overrideLateBindableName1` × 2 |

No judged case has an *extra* declaration-family diagnostic, and none of
these cases misses anything else (TS2589 instantiation-depth cases are not
this lane's and are excluded).

## 2. TS9026: an import kept only for an augmentation

`transformImportDeclaration` (`transform.go:2471`) elides an import none of
whose bindings is visible — unless `IsImportRequiredByAugmentation`
(`emitresolver.go:504`) says the imported file augments one of this file's
exports, in which case it keeps a bare `import "x"` and, under
`isolatedDeclarations`, reports TS9026 at the import. Only a **named-imports**
clause reaches that arm: a side-effect import, a default-only import and a
namespace import return earlier (`:2472`, `:2491`, `:2509`).

`IsImportRequiredByAugmentation` asks, for each symbol of the *parse-tree*
module symbol's exports, whether `getMergedSymbol(s) != s` and the merged
symbol has a declaration in the import's target file. This port merges
augmentations **in place** (`merge_module_augmentations`), so the test reads:
an export with a declaration in this file *and* one in the target. The first
half is what native's original table guarantees — a name an augmentation
*adds* lives only in native's merged clone — and without it an augmentation
adding a fresh name would answer `true` where native answers `false`.
`export *` re-exports are not walked (declines).

Native reports on the statement's *first* transform; a later late-painted
re-transform finds a visible binding and returns early, but the diagnostic
stays. Visiting the statement once, in order, reproduces that.

Measured: +1 case (`isolatedDeclarationErrorsAugmentation`), zero diagnostics
verdict changes elsewhere.

## 4. Remaining clusters, with what each needs

Every remaining declaration-emit case needs one of two pieces this port does
not have, and both are bigger than a box:

1. **`IsSymbolAccessible` / `getAccessibleSymbolChain`**
   (`symbolaccessibility.go`, 876 lines) as a faithful port, with
   `symbolToStringEx` for the error names (`'Foo'` / `"type"` in TS4023). The
   checker's naming code has *pieces* of it (`accessible_type_name_chain_at`,
   the shadowing predicate in `checker.rs`), each scoped to one printer
   caller and documented as not the whole walk.
2. **A structured walk of the inferred type** that reaches the symbols native
   tracks: computed property names keyed by `unique symbol`
   (`trackComputedName`, TS4023 × 3), expando property types (TS4032), the
   `this` type under an object literal type (TS2527, `declarationFiles`), an
   inaccessible `unique symbol` property (TS2527), a mapped type over
   `typeof sym` (TS4118). This port's `TypeData` carries **pre-printed text**
   and, for anonymous object types, no resolved member list
   (`types.rs`: `Named`/`Anonymous`), so the walk would have to be built on
   `get_property_names_of_type` and friends per shape.

TS2883 (12 cases) additionally needs **module-specifier generation into
`node_modules`** (`modulespecifiers/specifiers.go`,
`tryGetModuleNameAsNodeModule`, package `exports` / `typesVersions`): the
port's `module_specifier_for_symbol` (`checker.rs`) declines every
`node_modules` target. The `.types` baselines of the same cases want the same
specifier (`import("foo/node_modules/nested").MySpecialType` in
`declarationEmitUnsafeImportSymbolName`), so the specifier port pays twice.

The checker-side codes (§1, last row) belong to their checks' lanes.
