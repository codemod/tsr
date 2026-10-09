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
