# r6-printer: type-printer parity, third pass (`tsr-2zk`)

Lane `r6-printer`, successor of r5-printer3 (`r5-printer3.md`): `printing.rs`,
`signatures.rs`, `objects.rs`, `union_signatures.rs`, `literals.rs` and
`crates/tsr-scanner`. Pinned vendor `5b1047d`.

## 0. Base and method

Frozen base: `claude/beautiful-shannon-ar5gh0` `b18aec06` (main `17265fac`
plus bookkeeping). Diagnostics 12,238 cases; types 549,853 RIGHT / 843 GAP /
5,607 WRONG of 556,303 lines. Every number below is unfiltered against that
base. Losses are checked with `cut -f1,2` joins (`box-protocol.md` §5), and
slowcases runs on both dumps.

**Oracle.** `scripts/offline-cargo/build-tsgo.sh` builds the native tsgo. The
`.types` oracle is the pinned compiler test runner
(`go test -modfile=<tsgo.mod> -c ./internal/testrunner`), run on a probe
copied into `testdata/tests/cases/compiler` (r5-printer3 §1). Every native
claim below was read that way.

**Setup note.** PyPI answered 403, so `tomlkit` could not be installed. A
stdlib-only stand-in for assemble.py's `parse`, `inline_table` and `dumps`
lives in the session scratchpad and is not committed (r5-operators3 §4).
The `tomllib` parse is fed through a small TOML writer, and the vendored tree
built cleanly.

**Ir** is callgrind's total for `tsr -p <project> --noEmit --singleThreaded
--pretty false`. CLI output is compared byte for byte against the base binary.

## 1. A `declare global` member is visible to node reuse (item 1)

**Forcing constraint.** r5-declared4's print-arity WIP
(`r5-declared4-print-arity-WIP.diff`) makes a type reference print every
argument, as `typeReferenceToTypeNode` does. It lost three lines, and each
one has a return annotation written as `Iterator<X>`. Native reuses the
written node (`serializeReturnTypeForSignature`, `nodebuilderimpl.go:2023`)
and prints `() => Iterator<X>`. The port printed
`() => Iterator<X, any, any>`. A written `Generator<X>` was reused.

**Cause.** Signature construction carried the written return correctly, and
`written_annotation_text` was not involved. The reuse visitor refused the
name at the print site. `track_existing_leftmost_identifier`
(`trackExistingEntityName`, `nodecopy.go:317`) checks the found symbol with
`has_visible_declarations`. One of `Iterator`'s declarations is the
`interface Iterator<T, TResult, TNext>` inside `lib.es2025.iterator.d.ts`'s
`declare global { … }`. That lib file is a module (`export {};`).
node_reuse.rs's `is_declaration_visible` ports
`determineIfDeclarationIsVisible` (`emitresolver.go:131`), but its
`IsExternalModuleAugmentation` test only accepted a string-named module. So
`declare global` failed, the interface and the `var Iterator` inside it were
both invisible, and the whole reuse was refused. `Generator` has a single,
script-file declaration, so it was unaffected. The checker's other port of
the same function (`symbol_access.rs::is_external_module_augmentation`)
already includes the `global` arm.

**Fix (diff, not this lane's file).**
[`r6-printer-global-augmentation-visible.diff`](r6-printer-global-augmentation-visible.diff)
ports `IsExternalModuleAugmentation` (`ast/utilities.go:3567`, with
`IsAmbientModule` `:1652` and `IsModuleAugmentationExternal` `:1694`) into
node_reuse.rs. It reuses the existing `is_ambient_module_declaration`
(`unused.rs`, the same predicate). The test is
`crates/tsr-conformance/tests/global_augmentation_reuse.rs`: a user
`declare global { interface Pair<T, U = any> }` in a module file, where
`function g(): Pair<number>` prints `() => Pair<number>` natively. The base
prints `() => Pair<number, any>`, and the test fails without the diff.

**Measured** (owner: r6-nodereuse, `node_reuse.rs`):

| Applied on the base | Types | Diagnostics | slowcases | Ir domain-model | Ir generic-imports |
|---|---|---|---|---|---|
| base | 549,853 RIGHT | — | — | 1,090,900,622 | 343,081,467 |
| this diff alone | +0 / −0 | unchanged | clean | 1,090,902,385 (+0.0002%) | 343,071,223 (−0.003%) |
| this diff, then r5-declared4's WIP | **+37 / −0** | unchanged | clean | 1,093,278,178 (+0.22%) | 343,295,808 (+0.06%) |

CLI output is identical to the base in every row. On its own the diff moves
no corpus line. The base's display arity (§136) already printed
`Iterator<X>` short, which hid the refusal. With the WIP on top, the three
WIP losses (`generatorReturnExpressionIsChecked:0:0`,
`generatorTypeCheck64:0:11`, `types.asyncGenerators.es2018.2:0:121`) stay
RIGHT and the WIP's gains remain: +37 lines, 0 lost. The +0.22% Ir in that
row comes from the WIP (`declared.rs`), not from this diff. r6-declared
should weigh that cost when it lands the WIP.

**Apply order:** this diff, then `r5-declared4-print-arity-WIP.diff`.

**How this would be wrong.** If a declaration inside a non-lib
`declare global` became nameable where native refuses it, the result would
be a reused name native serializes. Native's `determineIfDeclarationIsVisible`
has the same arm, so that would need a different upstream rule.

## 2. Node-reuse printer leftovers (item 2)

### 2.1 Already RIGHT at the base

`divergentAccessors1` has no non-RIGHT line at `b18aec06`. r5-printer2 §4.1
(`type_literal_accessor_pair_at`) landed `addPropertyToElementList`'s
accessor arm. `numericStringNamedPropertyEquivalence:0:7`, `0:10` and `0:11`
are RIGHT too. r5-printer2 §4.2 (`merged_written_name`) and r5-declared4
§1.2 landed the object-literal and type-literal halves of the quoting rule.
Nothing is left to port for either case.

### 2.2 An unexported alias is not reused where it cannot be named

**Forcing constraint.** `declarationEmitPartialNodeReuseTypeReferences`:
file `a` writes `p2: PrivateSpecialString`, a non-exported
`type PrivateSpecialString = string`. File `b` prints `a.o`'s type. Native
prints `p2: string` and `bar: string` there. The port printed the alias
name, which cannot be named in `b`.

Native has one decision. `tryReuseExistingTypeNode` reaches
`trackExistingEntityName` (`nodecopy.go:317`). The name does not resolve at
`b`'s site, so it introduces an error, and `serializeTypeName`
(`nodebuilderimpl.go:436`) asks `IsSymbolAccessible(symbol,
enclosingDeclaration, meaning, false)` (`:451`). No accessible chain
reaches the alias. Its only container candidate, module `a`, has no export
that aliases it (`getContainersOfSymbol` → `getAliasForSymbolInContainer`,
`symbolaccessibility.go:280`). So the answer is `CannotBeNamed`, the reuse
fails, and the slot is serialized from its type, plain `string` (an alias to
an intrinsic carries no alias symbol).

The port had two holes:

1. `serialize_type_name` (node_reuse.rs) stood in for `IsSymbolAccessible`
   with only `hasVisibleDeclarations`, which a module-level alias passes
   (`IsLateVisibilityPaintedStatement` in a visible source file).
2. When the reuse is refused, signature printers fall back to
   `annotation_alias_text_at` (signatures.rs). That road re-emits an
   *erased* alias (an alias whose declared type is an intrinsic or a
   reduced singleton). Native has no such road. Its reuse is the visitor's
   alone, so this road has to apply the same refusal. It named the alias
   through `reference_text_at`, which does not ask accessibility.

**Port.** Both holes ask the checker's existing faithful `IsSymbolAccessible`
(`symbol_accessibility.rs`, `DeclarationEmitResolver::is_type_symbol_accessible`
/ `is_value_symbol_accessible`, `shouldComputeAliasesToMakeVisible` false).
The resolver is built per question. It owns only accessibility caches, so
nothing outlives the call and no new cache or side table is added. The cost
appears only on a refused or fallback reuse (see Ir below).

- Commit (this lane): `annotation_alias_node_at` refuses an inaccessible
  alias. Alone it moves **no** line, because the visitor still reuses the
  alias first. Types +0/−0, diagnostics unchanged, slowcases clean. Ir
  domain-model 1,091,897,193 (+0.09% against the base's 1,090,900,622),
  generic-imports 343,082,884 (+0.0004%). CLI output identical.
- Diff (r6-nodereuse's `node_reuse.rs`):
  [`r6-printer-serialize-type-name-accessible.diff`](r6-printer-serialize-type-name-accessible.diff).
  `serialize_type_name` asks the whole `IsSymbolAccessible`. It includes the
  test `crates/tsr-conformance/tests/foreign_site_alias_reuse.rs` (native
  lines read from the oracle).

**Measured, diff on top of the commit**, unfiltered against the base:
types **+21 / −0**, diagnostics unchanged, slowcases clean. Ir domain-model
1,090,853,332 (−0.004% against the base), generic-imports 343,063,349
(−0.005%). CLI output identical. Gains:

- `declarationEmitPartialNodeReuseTypeReferences:1:1,1:2,1:4` (`b.ts`);
- `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks{1,2,3}`
  (8 lines);
- `exportEqualErrorType` and `exportEqualMemberMissing` (3 lines each);
- `aliasOnMergedModuleInterface:0:3,0:5`;
- `declarationEmitUnnessesaryTypeReferenceNotAdded(target=es2015):0:0,0:5`.

Each of the other cases had reused a written name the site could not
reach. For example, `exportEqualErrorType` printed
`server.connectExport` where native's accessible chain names
`connect.connectExport`.

**Remaining in the case: `c.ts` (3 lines).** Native prints
`import("./a").N.SpecialString`. `c.ts` imports neither `N` nor the module
namespace, so `symbolToTypeNode` reaches `N` only through an import type.
The port prints `N.SpecialString`. This is not reuse: the type's own print
has the same gap (`j : N.I` where native prints `j : import("./a").N.I`,
probed). The site namer (`reference_text_at` / `symbol_chain`,
`checker.rs`, main's) never emits an import type. Routed to main.

**How this would be wrong.** If the accessibility walk refused a name that
native reuses, some line now RIGHT would turn WRONG. The unfiltered run
shows none. If `annotation_alias_text_at` were retired in favour of the
visitor alone, as native has it, this gate would go with it.
