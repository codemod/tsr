# r4-declemit — declaration-emit accessibility diagnostics

Lane notes for round-4 box `r4-declemit` (`bd tsr-2zk.904`). Pinned upstream:
`vendor/typescript-go` @ `5b1047d`.

## 1. What was missing, measured

Box baseline (integration head `b23dd3d`, `diagverdictdump`): **24** judged
cases miss *only* declaration-family codes, and no judged case misses one
alongside another difference. By root cause:

| Producer upstream | Codes | Cases |
|---|---|---|
| `checkEntityNameVisibility` on a **written** name (`transform.go:1697`) | 4025, 4081 | 2 — `declarationEmitVarInElidedBlock`, `declarationEmitInvalidExport` |
| node builder `TrackSymbol` → `isSymbolAccessible` (inferred types) | 4023, 4025, 4032 | 6 — `declarationEmitComputedPropertyNameSymbol1/2`, `declarationEmitReadonlyComputedProperty`, `jsDeclarationsTypeReassignmentFromDeclaration2`, `globalThisDeclarationEmit`, `declarationEmitExpandoPropertyPrivateName` |
| node builder `ReportLikelyUnsafeImportRequiredError` (module specifiers) | 2883 | 6 |
| node builder `ReportPrivateInBaseOfClassExpression` (`nodebuilderimpl.go:2665`) | 4094 | 2 — `privateFieldsInClassExpressionDeclaration`, `declarationEmitMixinPrivateProtected` |
| node builder `ReportInaccessibleThisError` / `…UniqueSymbolError` | 2527 | 2 |
| node builder `ReportNonSerializableProperty` | 4118 | 1 |
| **checker**, not declaration emit | 4109/4110, 4105, 4111, 4113 | 5 — other lanes |

The `privacy*` family that dominates upstream's TS4xxx baselines is excluded
from the suite (configuration-varied or expecting nothing), so it is not in
these counts — but every `EMPTY_RIGHT` privacy case is a loss guard: a wrong
"not visible" answer there would fail it.

## 2. Where the walk lives, and why not in `tsr-declarations`

Upstream raises the written-name errors from inside `DeclarationTransformer`.
This port already has that transform (`crates/tsr-declarations`), so the
faithful home is there. It was not used, for two reasons:

- **Ownership.** `tsr-declarations` is not this lane's (round-4 rules); the
  integrator serializes changes to it.
- **Its resolver is syntactic and its walk carries no node ids.** Its
  `EmitResolver` (`resolver.rs`) answers from syntax alone, its
  `is_declaration_visible` takes a `Statement`, and its type nodes are copied
  through the factory without visiting. Adding `IsEntityNameVisible` there
  means threading `NodeId`s and a checker-backed resolver through the trait,
  which every implementation and the `.d.ts` emit suite would carry.

So `tsr_dts::accessibility` ports the **diagnostic** half of the transform
over `NodeId`s: `visit`, `visitDeclarationStatements`,
`transformTopLevelDeclaration`, `visitDeclarationSubtree`, the
`setupDiagnosticContext` state, `ensureType`'s written-annotation arm,
`ensureParameter`, `updateParamList`, the late-painting queue, and
`diagnostics.go`'s message selection. The checker half
(`DeclarationEmitResolver` in `crates/tsr-checker/src/symbol_access.rs`) ports
`EmitResolver.isDeclarationVisible`, `determineIfDeclarationIsVisible`,
`PrecalculateDeclarationEmitVisibility`/`markLinkedAliases`,
`isEntityNameVisible`, `hasVisibleDeclarations` and
`IsImplementationOfOverload`.

**What would change this:** when `tsr-declarations` grows a checker-backed
resolver (it needs one for `CreateTypeOfDeclaration` anyway), the walk should
move into its transform and this module be deleted — two walks over the same
tree that must agree on what is emitted is a standing drift risk. The
falsifier for keeping it here is any case where the two disagree on whether a
declaration is emitted.

### Why the emit resolver keeps its own visibility table

`node_reuse.rs` already has `is_declaration_visible`/`has_visible_declarations`
for type printing, without state. Native keeps `declarationLinks` on
`EmitResolver`, and declaration emit *mutates* them: `addVisibleAlias` and
`markLinkedAliases` paint a non-exported statement visible, which is what
makes `interface I {}` in a module emittable once an exported declaration
names it. Reusing the stateless copy would either lose the painting or leak
it into type printing. The resolver owns its table (port-convention record on
`DeclarationEmitResolver`).

## 3. Declines (each skips work upstream does; none adds work)

Measured consequence: the producer fires in exactly the two target cases
across the whole judged corpus (actual-column diff baseline → after).

- **Inferred types** (`CreateTypeOfDeclaration`,
  `CreateReturnTypeOfSignatureDeclaration`, `CreateTypeOfExpression` for a
  class `extends <expr>`): need the node builder's `SymbolTracker`. That is
  every row of §1 but the first. The tracker belongs in the type printer
  (`printing.rs` and the `type_to_string` family, not owned by any round-4
  box); see §4.
- **Parameters `RequiresAddingImplicitUndefined` could send to the node
  builder**: any initialized parameter, or an optional parameter property.
- **Dynamic member names** (`HasDynamicName` → `IsLateBound` → `checkName`):
  the whole member is skipped.
- **Binding patterns** (`recreateBindingPattern`, `walkBindingPattern`,
  `visitBindingName`'s computed keys).
- **JavaScript files** (`TryJSTypeNodeToTypeNode`) and files under
  `node_modules/` (standing in for `IsSourceFileFromExternalLibrary`, which
  the program does not record).
- **`typeof this`**: `isEntityNameVisible`'s `this` arm answers `NotResolved`
  here; native answers `Accessible` or the same `NotResolved`, neither of
  which reports.
- **Names the binder's `resolve_name` cannot reach** (globals, some namespace
  exports) come back `NotResolved` and report nothing.
- `IsImplementationOfOverload` reads `getSignaturesOfSymbol` as the symbol's
  function-like declarations — exact for TypeScript files.

## 4. Remaining clusters, with what each needs outside owned files

All remaining declaration-emit cases (16) need a `SymbolTracker` in the type
serializer: a callback the node builder invokes at `TrackSymbol`
(`nodebuilderimpl.go:1062`, `:3145`), at class-expression members under
`FlagsWriteClassExpressionAsTypeLiteral` (`:2665`), at inaccessible `this` /
`unique symbol` (`:3322`, `:3356`), at non-serializable properties (`:2510`)
and at module-specifier generation (`:709`). This port's serializer prints
strings (`printing.rs`, `checker.rs`'s chain printer) and has no such hook,
and declaration emit has no `CreateTypeOfDeclaration` that drives it. The
hook is a hub-file change (`printing.rs`, `checker.rs`); the
`SymbolTrackerImpl` side (`tracker.go`) and the message selection it needs
are already here (`Walk::diagnostic_for_node` covers the private-name arms;
the `CannotBeNamed`/module arms are a mechanical extension).
