# r4-helpers — external emit helpers and type-only names in value position

Round-4 lane notes (`tsr-2zk.21`, `tsr-2zk.41`, `tsr-2zk.6.3`). Box baseline:
integration head `78bde77`. Every number below is unfiltered against that
baseline unless it says otherwise.

## §1. Construct-level `checkExternalEmitHelpers` requests (`tsr-2zk.21`)

### The forcing constraint

`checkExternalEmitHelpers` (`checker.go:28576`) has 26 call sites upstream,
one per construct whose downlevel emit calls a `tslib` helper. The names lane
(r3) ported the function and the import/export sites; every other site was
missing, so a file whose first helper request is an `async` function, a
`using` declaration, an object rest element or a decorator never reported
TS2354 / TS2343. r3-names measured those sites as a patch
(`names-emit-helpers.diff` on `claude/beautiful-shannon-ar5gh0-r3-names`).

### What was built

`Checker::check_construct_emit_helpers` (`emit_helpers.rs`), one call at the
top of `check_node`. Arms, each under its owner's condition:

| Upstream site | Condition |
|---|---|
| `checkSignatureDeclaration` (`:2731-2741`) | `GetFunctionFlags`: async with a body; async generator `< ES2018` → `__await`+`__asyncGenerator`; plain async `< ES2017` → `__awaiter` |
| `checkVariableDeclarationList` (`:5774`) | `using` / `await using` `< ESNext` |
| `checkVariableLikeDeclaration` (`:5821`) | object-binding rest element `< ES2018` → `__rest` |
| `checkDecorators` (`:6031-6038`) | first decorator; legacy → `__decorate` (+ `__param` on a parameter); standard `< ESNext` → `__esDecorate`+`__runInitializers` |

**Why one walk-top call rather than a call in each owner.** The owners are
hub functions of other lanes (`check.rs`'s signature, variable and decorator
checks). The request is a pure side query — it reads options and syntax and
writes only the per-file links `emit_helpers.rs` owns — so its position in
the owner does not matter except for *order*: the first request in a file is
where a missing helper is reported. `check_node` is pre-order in source
order, which is the order upstream's `checkSourceElement` reaches these
constructs. Would be wrong if a construct upstream checks *deferred*
(`checkNodeDeferred`) made the first request of a helper that a later
non-deferred construct also requests; no corpus case has that shape
(the loss check below is the evidence).

**Not ported in this arm.** `checkDecorators`' `ast.NodeCanBeDecorated` half
of the entry guard: its port (`node_can_be_decorated`) is private to
`grammar.rs`, which this lane does not own. A node it rejects already
carries TS1206/TS1249; the cost is a possible extra TS2343 beside it. The
`__setFunctionName` / `__propKey` arms of `checkDecorators` and
`checkClassExpressionExternalHelpers` are not built yet.

### The harness directive is outside the lane

`@importHelpers` was never mapped into `CompilerOptions` by
`apply_test_directives` (`crates/tsr-conformance/src/trace_case.rs`), so the
directive-driven cases never enable the option; only the three
tsconfig-driven `tslib*` cases do. The one-line fix is
`docs/parity/notes/r4-helpers-harness.diff`.

### Measured (box baseline `78bde77`)

- **Commit alone:** diagnostics **+3** (`tslibMissingHelper`,
  `tslibMultipleMissingHelper`, `tslibNotFoundDifferentModules`), 0 lost;
  types 0 lost.
- **With the harness diff:** diagnostics **+8** (the three above plus
  `ctsFileInEsnextHelpers`, `esModuleInteropTslibHelpers`, `importHelpersES6`,
  `exportAsNamespace_missingEmitHelpers`, `usingDeclarationsWithImportHelpers`),
  0 lost; types 0 lost. `awaitUsingDeclarationsWithImportHelpers` stays
  wrong at column 11 vs 5: this parser's `VariableDeclarationList` span for
  `await using` starts at `using` (parser lane).
- **Perf** (median child CPU, new/old, 41 samples): `domain-model` 1.016,
  `generic-imports` 1.001. The arm returns on `!import_helpers` first.
