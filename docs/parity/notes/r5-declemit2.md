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
| node builder `ReportPrivateInBaseOfClassExpression` (`:2665`) | 4094 | 2 — **converted, §3** |
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

## 3. TS4094: a class expression written as a type literal

Declaration emit builds types with `FlagsWriteClassExpressionAsTypeLiteral`
(`transform.go:216`). With it, `createAnonymousTypeNode`
(`nodebuilderimpl.go:2805`) expands the static side of any class whose value
declaration is a class *expression* — unconditionally, through the
`!IsClassDeclaration` disjunct — and `createTypeNodesFromResolvedType`
(`:2660`) calls `ReportPrivateInBaseOfClassExpression` for each property that
is private, protected (`getDeclarationModifierFlagsFromSymbol`) or
`#private` (`IsPrivateIdentifierSymbol`, reported under `SymbolName`). The
static side's construct signature returns the instance type, which
`typeToTypeNode` (`:3047`) expands too, because a class expression's symbol is
never value-accessible from outside its own body.

**Where it lives.** The tracker half (message, `errorNameNode` / fallback
stack location, `tracker.go:131`) is in `tsr_dts::accessibility`; the
node-builder half is `DeclarationEmitResolver::inferred_type_reports`
(`symbol_access.rs`), reached through a new `AccessibilityResolver` method.
The walk asks it at the three places native calls the node builder for an
*inferred* type in a TypeScript file:

- `ensureType` on a variable declaration with no annotation
  (`CreateTypeOfDeclaration`, `transform.go:1667`), located at the name;
- `transformExportAssignment`'s `_default` arm (`:1250`), located at the
  assignment (pushed as the fallback node; an export assignment has no name);
- `transformClassDeclaration`'s `extends <non-entity expression>` arm
  (`CreateTypeOfExpression`, `:2018`), located at the class name, or at the
  class when it has none.

**Why not a hook in the type printer.** r4-declemit §4 said the general
`SymbolTracker` belongs in the serializer (`printing.rs`, `checker.rs`). That
still holds for `TrackSymbol`, whose call sites are wherever a symbol is
named. This arm is different: the `.types` printer never takes it (it prints
`typeof C`; the flag is declaration-emit only), so no printer path exists to
hook, and the decision reads only the class symbol and its two property
lists. **What would change this:** a declaration-emit node builder in the
serializer; this function should then become the body of its
class-expression arm, not a second walk.

**Declines** (each misses errors, never invents one):

- Only the *top-level* inferred type is read. A class expression nested in a
  union, a property type or a signature is reached by native's recursion, not
  here.
- A class *declaration* whose name is inaccessible takes the same expansion
  natively (the `IsSymbolAccessible` disjunct); it needs
  `getAccessibleSymbolChain` (§4).
- `CreateReturnTypeOfSignatureDeclaration`, property declarations and
  parameters are not asked.

Measured: +2 cases (`privateFieldsInClassExpressionDeclaration`,
`declarationEmitMixinPrivateProtected`); see the commit for the loss checks.

One fact about this port found on the way: the binder leaves
`value_declaration` unset on a class expression's symbol; the arm reads the
symbol's first declaration in its place (a class expression's symbol has
exactly one).

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

### 4.1 The `this` arm, tried and stopped

TS2527 in `declarationFiles` (4 diagnostics) is the arm closest to §3: a
`this` type reached while `FlagsInObjectTypeLiteral` is set
(`nodebuilderimpl.go:3352`, the flag set at `:2741` around a type literal's
members) reports `ReportInaccessibleThisError`, located at the member name.
Its error set (`x1`, `x3`, `f1`, `f3`, not `x2`/`x4`/`f2`/`f4`) is exactly
"`this` under an object type literal, possibly inside an array", so the walk
must descend object-literal member types and array element types while
skipping a lone call signature, which native writes as a function type with
the flag unset (`:2705`).

Stopped before writing it, for one case: an object literal's members here are
`objects::Member` values carrying **printed** member text, with the semantic
types in a separate `anonymous_properties` image read through
`property_type` slots, and array element types live in yet another
representation. Every arm of the walk would be a separate adapter onto a
different side table, each a loss risk in the EMPTY_RIGHT `privacy*` and
class-member cases, for +1 case. It is the first arm to build once (2) above
exists as a typed member walk.

### 4.2 The checker-side codes, routed

| Code | Native producer (`checker.go` @ `5b1047d`) | In this port |
|---|---|---|
| 4111 | `checkPropertyAccessExpressionOrQualifiedName` (`:11362`) | no producer |
| 4113 / 4114 | `checkMemberForOverrideModifier` (`:4764`, `:4772`) | `heritage_conformance.rs` has it; the late-bindable / computed-name members are missed |
| 4105 | `checkIndexedAccessIndexType` (`:8248`) | no producer |
| 4109 / 4110 | `getTypeArguments` circularity (`:21924`) | no producer |

None of these is in a file this lane owns.
