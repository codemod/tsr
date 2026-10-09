# r6-specifiers2: export-specifier aliases, `processEnding`'s file probe, and the `.16.77` leftovers (tsr-2zk.1149)

Round-6 box, succeeding r6-specifiers (`docs/parity/notes/r6-specifiers.md`,
batch BS). Owned: `crates/tsr-checker/src/module_specifiers.rs`,
`crates/tsr-checker/src/resolution.rs`, `crates/tsr-module`. Everything else
ships as a measured diff in this directory. tsgo `5b1047d`; every expectation
below was checked against a native `tsgo` built from that pin by
`scripts/offline-cargo/build-tsgo.sh`.

Base frozen at `5e4d21b` (tip of `claude/beautiful-shannon-ar5gh0` when the
box started; BS had not landed). Base: types 556,303 aligned lines;
diagnostics 12,238 rows.

## 1. `export { T }` read through an import — `r6-specifiers2-pure-alias-type-reference.diff`

### 1.1 The cause

r6-specifiers §1's probe, with no symlink:

```ts
// b.ts
type T = number; export { T }
// app.ts
import { T } from "./b";
let w: T;
const q: string = w;   // native: TS2322 + TS2454; TSR (base): nothing
```

Not `export_specifier_target` (`symbols.rs`): traced, the import alias
resolves to the export specifier's alias, and that one resolves to the type
alias `T`, both correctly. The `any` comes from the **consumer**:
`get_type_from_type_reference`'s §491 road (`declared.rs`, r6-declared2's
file) calls `resolve_alias` **once** for an unrenamed import specifier, lands
on the export specifier's symbol (flags `ALIAS` only), fails the `TYPE` test
and answers `errorType`.

Native's `resolveAlias` (`checker.go:16266`) does not stop there: when the
target `IsNonLocalAlias(target, Value|Type|Namespace)` (`ast/utilities.go:2608`
— a pure alias, or an alias with `Assignment`) it continues through
`resolveIndirectionAlias` (`:16293`). This port's `resolve_alias` is the
one-step `getTargetOfAliasDeclaration` memo, and each consumer chooses how far
to follow it.

`export { T as U }` / `import { U }` worked on the base only by accident of a
different road (the renamed-specifier primitive arm, which already calls
`resolve_alias_fully`).

### 1.2 The diff

`declared.rs`, the §491/§493 alias road: after `resolve_alias`, follow the
target while it is a pure alias (`IsNonLocalAlias`'s exact test), as
`resolveIndirectionAlias` does. Bounded at 8 links, the same policy as
`resolve_alias_fully`. Plus `crates/tsr-compiler/tests/r6_specifiers2_export_specifier_alias.rs`
(the probe, and a renaming chain `C → B → class A` that must report TS2741).

**A name gate was measured and refused.** The §491 comment says the
unrenamed specifier's local name and the target's name "agree by
construction"; a followed chain breaks that (`export { MyClass as MyClass2 }`).
The first version declined (answered `errorType`) when the followed target's
name differed from the local name. Measured: +18 types, +1 diagnostics case.
Without the gate: **+28 types, +4 diagnostics cases**, 0 lost either way. The
printer already names the type at its site (r6-accessible's
`IsTypeSymbolAccessible` arm): native prints `J` for `import { J }` of
`interface I {}; export { I as J }` (tsgo `--declaration`: `q: J[]`), and so
does TSR without the gate. `moduleResolutionWithSymlinks{,_withOutDir}` are
exactly that shape (`let y: MyClass2` prints `MyClass`, the name in scope),
and converting them needs the gate gone. Native has no such gate.

### 1.3 Measured (against `5e4d21b`, unfiltered)

- **Types +28, 0 lost**: `declarationsForIndirectTypeAliasReference` 8,
  `moduleResolutionWithSymlinks` 4, `moduleResolutionWithSymlinks_withOutDir` 4,
  `noCrashOnImportShadowing` 2, `emitDecoratorMetadata_isolatedModules` ×2
  modules 2 each, `exportDeclaration` ×2 1 each, `generic` 1, `enums` 1,
  `chained` 1, `decoratorMetadataTypeOnlyExport` 1.
- **Diagnostics +4, 0 lost**: `mergeSymbolReexportInterface`,
  `moduleResolutionWithSymlinks`, `moduleResolutionWithSymlinks_withOutDir`,
  `chained`.
- `slowcases` clean on both dumps.
- Ir (`valgrind --tool=callgrind`, release `tsr -p … --singleThreaded
  --noEmit`): dm 1,090,963,052 → 1,090,894,305 (−0.006%), gi 343,064,681 →
  343,072,112 (+0.002%).

**The corpus count the brief asked for**, measured with a throwaway
instrument on the road (not committed), over the whole types run: on the base
the road met an import whose one-step target is a pure alias **50 times**
(`B` 12, `T1` 8, `Row2` 5, `zzz` 4, `StringHash{,2}` 4 each, `MyClass2` 4, `A`
4, and `X`, `TypeFlag`, `SymbolFlags`, `Foo`, `D` once each). Every one of them
answered `errorType`. With the diff, **0** stop on a pure alias: 483 road
queries reach a type, and 5 reach a target with no type meaning
(`zzz` ×4 and `A` ×1, a value-only export, where `errorType` is native's
answer). The `Row2` hits now resolve, but their lines stay WRONG for another
reason (§4.2).

Owner: r6-declared2 (`declared.rs`). Independent of every other diff here.

## 2. Default type arguments in `ObjectAssignedDefaultExport` — `r6-specifiers2-reused-module-member.diff`

### 2.1 Not elision: node reuse

r6-specifiers §4 read the six lines (`StyledComponent<"div">` where TSR
printed `StyledComponent<"div", DefaultTheme, {}, never>`) as the node
builder dropping type arguments equal to their defaults. **Native has no such
elision**: the alias arm (`nodebuilderimpl.go:3366`) maps every
`alias.TypeArguments()`. The short form is the **written return annotation,
reused**: `div: (a: TemplateStringsArray) => StyledComponent<"div">`
prints its signature through `serializeReturnTypeForSignature`, which reuses
the annotation node (`tryReuseExistingTypeNode`). At the importing file
`StyledComponent` is not in scope, so `tryVisitTypeReference`
(`nodecopy.go:416`) takes the `introducesError` arm: `serializeTypeName`
(`:436-449`) names the symbol through `symbolToTypeNode` and keeps the
**written** type arguments: `import("styled-components").StyledComponent<"div">`.
tsgo `--declaration` on a reduced probe agrees:
`d: (a: TemplateStringsArray) => import("sc").SC<"div">`.

TSR already reuses the node (it prints `SC<"div">` in the declaring file).
At the other file, `serialize_type_name` (`node_reuse.rs`) names a module
member only through `parameter_source_symbol_name_at`, a bounded walk of the
scope tables at the site. That walk finds nothing for an unimported module's
member and answers `None`, so the whole reuse declines (`unnameable`) and the
type is serialized structurally, with every argument.

### 2.2 The diff

`node_reuse.rs` (no owner this round), `serialize_type_name`'s module-member
arm: when the table walk declines, name the symbol through `symbol_chain`
(the printer's `getSymbolChain` port, which spells `import("…")` for an
unreachable module), as the arm's non-module branch already does. Plus
`crates/tsr-conformance/tests/r6_specifiers2_reused_module_member.rs` (the
reduced probe, against tsgo's answer).

**Refused first: `reference_text_at`.** It printed the same text, measured
+15 types and 0 lost, but failed
`signatures::tests::parameter_source_views_qualify_bound_names_and_reject_unloaded_imports_without_work`:
a full print at `importedView` changed the type store, and that test asserts
that printing is read-only. `symbol_chain` passes it.

### 2.3 Measured (against `5e4d21b`, unfiltered)

- **Types +14, 0 lost**: `declarationEmitObjectAssignedDefaultExport` 6 (all
  six lines), `duplicatePackage` 3 (the parameter slots `(x: import("a/node_modules/x").default) => void`),
  `declarationEmitPartialNodeReuseTypeOf` 3,
  `importShouldNotBeElidedInDeclarationEmit` 2.
- Diagnostics: no change.
- `slowcases` clean; `cargo test --workspace --release` green; clippy reports
  the same 12 pre-existing stable-toolchain errors with and without it, none
  in these lines.
- Ir: dm 1,090,963,052 → 1,090,926,551 (−0.003%), gi 343,064,681 →
  343,096,295 (+0.009%).

`declarationEmitObjectAssignedDefaultExport`'s remaining line (2:19) is
r6-specifiers §4's: `NonReactStatics` takes the alias
`hoistNonReactStatics.` (the accessibility walk, `tsr-2zk.39`). Owner of the
diff: whoever takes `node_reuse.rs`; it touches no other file. Independent
of §1.
