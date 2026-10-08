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

## §2. TS2693 for a primitive name in a file with parse errors (`tsr-2zk.6.3`)

### The forcing constraint

`checkAndReportErrorForUsingTypeAsValue` (`checker.go:1662`) reports TS2693
for `any`/`string`/`number`/`boolean`/`never`/`unknown` in a value position
whether or not the file parsed cleanly; the checker runs on recovered trees.
`check_value_identifier` (`check.rs`, the TS2693 primitive arm) declined
all six names in any file with a parse error. That gate was §949's
(`checker-notes-diag2.md`): the first build without it reported 48 extra
lines in recovered files, and gating removed them along with 29 right
lines, with the case count unchanged. Seven of the 23 TS2693-missing cases
at this baseline are parse-error fixtures whose baselines carry TS2693
(`autoLift2`, `createArray`, `staticsInAFunction`,
`overloadingStaticFunctionsInFunctions`, `classMemberWithMissingIdentifier2`,
`parserUnterminatedGeneric2`, `privateIndexer2`).

### Decision

Remove the gate; keep the `!upstream_six` decline (`void`, `object`,
`symbol`, `bigint` are not in upstream's `isPrimitiveTypeName`). The parser
has moved since §949: re-measured, the 48 extras are gone.

**Alternative rejected:** keep the gate and special-case the recovered
shapes — a heuristic over parser output, which §3a of the box protocol
rejects.

### Measured (box baseline `78bde77`, on top of §1)

Diagnostics **+7** (`autoLift2`, `classMemberWithMissingIdentifier2`,
`createArray`, `overloadingStaticFunctionsInFunctions`, `staticsInAFunction`,
`parserUnterminatedGeneric2`, `privateIndexer2`), 0 lost; types unchanged.
Two new extra lines in cases already wrong, both parser divergences:
`parseErrorIncorrectReturnToken` (4,15) — this parser recovers
`type F2 = (n: number): string` with `number` in a value position, upstream
with `string` (4,24); `mappedTypeProperties` (18,21) — this parser reports a
parse error upstream does not, after `[placeType in PlaceType]?: void;`, and
recovers `model(duration: number)` with `number` as a value. Owner: parser.
Perf (21 samples): `domain-model` 1.021, `generic-imports` 0.978.

**Would be wrong if** a later unfiltered run shows TS2693 extras
concentrated in parse-error files whose upstream baselines have none; then
the divergent recovery is the parser's to fix, not a reason to restore the
gate.

## §3. The expression-level helper requests (`tsr-2zk.21`)

Five more `checkExternalEmitHelpers` sites, in the same walk-top arm as §1:

| Upstream site | Condition |
|---|---|
| `checkPropertyAccessExpressionOrQualifiedName` (`:11268-11278`) | `x.#p` when private names are downleveled (`< ES2022`, `< ESNext`, or not `useDefineForClassFields`): a write → `__classPrivateFieldSet`, a read → `__classPrivateFieldGet`, compound → both (`getAssignmentTargetKind`) |
| `checkInExpression` (`:13081-13086`) | `#p in o`, same condition → `__classPrivateFieldIn` |
| `checkForOfStatement` (`:4036-4045`) | `for await` whose `getContainingFunctionOrClassStaticBlock` is an async function `< ES2018` → `__asyncValues` |
| `checkYieldExpression` (`:10971-10978`) | `yield*` in an async generator `< ES2018` → `__await`+`__asyncDelegator`+`__asyncValues` |
| `checkObjectLiteralDestructuringPropertyAssignment` (`:12619-12626`) | `{ ...r } = v`, the spread last, `< ES2018` → `__rest` |

`GetFunctionFlags` is now one function (`function_flags`), shared with §1's
signature arm.

### Order: operand first where upstream checks the operand first

Three of these sites run *after* their operand is checked upstream:
`checkPropertyAccessExpression` checks `left` before
`checkPropertyAccessExpressionOrQualifiedName`; `checkInExpression` runs
after both operands; `checkYieldExpression` after its operand. The walk is
pre-order, so `this.#a.#b` would request at the outer access first, while
upstream reports a missing `__classPrivateFieldGet` at the inner one.
`request_operand_emit_helpers_first` replays the operand subtree's requests
before the site's own. A request is idempotent once made (the per-file
mask), so the walk's later visit of that subtree is silent. Cost: a subtree
walk per such site, only under `importHelpers`.

**Not replayed:** the destructuring rest. Upstream checks the assignment's
right operand before the target's properties (`checkDestructuringAssignment`
receives `checkExpression(right)`); here the spread's request comes first.
It matters only when the right operand also requests `__rest` (a nested
destructuring assignment) — no corpus case; recorded rather than built.

### Verified against native

`tsgo` built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`)
and TSR give identical TS2343 output (8 diagnostics: get, set, in,
async generator ×2, for-await, `yield*`, object-literal rest) on a
probe with an empty `tslib`, `target: es2015`, `useDefineForClassFields:
false`.

### Measured (box baseline `78bde77`, on top of §1-§2)

With the harness diff: diagnostics **+2** (`privateNameEmitHelpers`,
`privateNameStaticEmitHelpers`), 0 lost; without it, no change and 0 lost.
Types unchanged. Perf (41 samples): `domain-model` 1.008,
`generic-imports` 1.017.

Still unported: `checkDecorators`' `__setFunctionName` / `__propKey` arms,
`checkClassExpressionExternalHelpers`, and `markDecoratorAliasReferenced`'s
`__metadata` request.

## §4. An alias matches a meaning by its target's flags (`tsr-2zk.6.3`)

### The forcing constraint

`import Sammy = require("./m")` where `m` has `export = Sammy` and `Sammy`
is an interface: `Sammy()` is TS2693 upstream and was TS2708 here.
Upstream's `onFailedToResolveSymbol` cascade asks
`resolveSymbol(resolveName(…, NamespaceModule))` first; `resolveName`'s
`getSymbol` returns an alias only when `getSymbolFlags(alias)` — the flags of
the whole alias chain — intersect the meaning (an interface does not), so
the namespace arm declines and the type-as-value arm reports TS2693.
This binder's `resolve_name` answers an alias for any meaning, so
`resolve_symbol_under` returned the alias directly and the namespace arm
fired. Verified against the pinned `tsgo` (TS2693 at the call).

### What changed (`meaning_mismatch.rs`)

- `resolve_symbol_under`: a found symbol that carries the meaning is
  returned; an alias is returned (resolved, `resolveSymbol`) only when
  `get_symbol_flags` intersects the meaning; anything else declines.
- The type-as-value arm resolves through `resolve_symbol_under` too and reads
  `get_symbol_flags` of the result, as upstream's `getSymbolFlags(symbol)`.

**Accepted limitation.** Upstream's `getSymbol` miss continues the scope walk
outward; this declines at the first scope that holds the alias. A shadowed
outer namespace of the same name would be missed. The binder walk is not
this lane's file; no corpus case has the shape.

### Measured (box baseline `78bde77`, on top of §1-§3)

Diagnostics **+2** (`typeUsedAsValueError2`,
`exportAssignmentOfDeclaredExternalModule`), 0 lost; types unchanged.
Line level: `es6ExportEqualsInterop` 4 wrong lines fewer (2 TS2708 → TS2693),
`exportNamespace9` 2 fewer, `allowImportClausesToMergeWithTypes` 3 wrong
lines → 1 — the remaining one is TS2749 where TS2709 was before, at
`const x: zzz` whose type meaning (interface merged with an import alias
re-exported as `default`) this port does not resolve; that miss predates
this change. Perf (21 samples): `domain-model` 0.981, `generic-imports`
1.019.

## §5. `maybeMappedType`: TS2690 for `{ [K]: T }` (`tsr-2zk.6.3`)

`checkAndReportErrorForUsingTypeAsValue` picks TS2690 ("Did you mean to use
'P in K'?") over TS2693 when `maybeMappedType` (`checker.go:1708`) holds:
the name is the computed key of the **only** member of a type literal and
`getDeclaredTypeOfSymbol` is a union every member of which
`isTypeAssignableToKindEx(…, StringOrNumberLiteral, strict)` admits. The
port carried only the syntactic half and declined the whole shape, recorded
as "a type question this port cannot ask here". It can now:
`get_declared_type_of_symbol` and `is_type_assignable_to_kind`
(`operator_operands.rs`, whose `NumberLike` arm is what admits `number` for
the `NumberLiteral` bit, as upstream's) are both available. Also moved to
upstream's position — after the value test and `export =` guard rather than
before the lookup.

The answer is read as `!= NotRelated`: upstream's `isTypeAssignableTo` is a
boolean whose `Maybe` counts as related.

Measured (on top of §4): diagnostics **+1** (`typeUsedAsTypeLiteralIndex`:
three TS2690 and one TS2693, the two-member literal), 0 lost, no other line
moved in any TS2690/2693/2708/2709/2749/2304 case; types unchanged. Perf
(21 samples): `domain-model` 1.005, `generic-imports` 0.999.
