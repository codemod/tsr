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

## 3. `tsr-2zk.1114` remainder (item 3)

### 3.1 A literal at a mutable location reads the instantiated contextual type

**Forcing constraint.** `arrayLiteralInference` (5 lines):
`const m: Map<AppType, AppStyle[]> = new Map([[AppType.Standard, […]], …])`.
Native infers `Map<AppType.AdvancedList | AppType.Standard |
AppType.Relationship, …>`. The port inferred `Map<AppType, …>`, because each
member literal was widened to its enum before inference saw it.

`checkExpressionForMutableLocation` (`checker.go:13878`) asks
`getWidenedLiteralLikeTypeForContextualType(t,
instantiateContextualType(getContextualType(node, None), node, None))`.
Without `ContextFlagsSignature`, `instantiateContextualType`
(`checker.go:30817`) incorporates only the inference context's
**return mapper**. The annotation `Map<AppType, …>` infers `K = AppType`
there, so the element's contextual type `K` reads as the enum `AppType`, a
union of member literals, and `isLiteralOfContextualType` keeps
`AppType.Standard`. The port passed the raw contextual type `K`, which is
unconstrained, so `isLiteralOfContextualType` answered `false` and the
literal widened.

**Port.** `check_expression_for_mutable_location` (objects.rs) puts the
contextual type through `instantiate_contextual_type_without_signature`
(contextual.rs). That function is the existing port of exactly this
non-Signature form, already used by `checkArrayLiteral`'s tuple-context read.
The §946 uninstantiated retry is left as it was. No cache or side table is
added.

**Measured**, unfiltered against the base: types **+5 / −0**
(`arrayLiteralInference:0:26,0:28,0:29,0:40,0:54`; the case converts),
diagnostics unchanged, slowcases clean, CLI output identical. Ir:
domain-model 1,090,870,104 and 1,091,914,416 on two runs of the same binary,
against the base's 1,090,902,888 and 1,090,897,313. generic-imports
343,083,992 / 343,084,367 against 343,079,364 / 343,080,143 (+0.001%).

*Noise correction.* Re-running one binary under callgrind moves domain-model
by about 1.0M Ir (≈0.1%). The commit binary of §2 measured 1,091,897,193
once and 1,090,864,817 on a second run. §2's "+0.09%" for the commit alone
is therefore within that noise. Treat it as ±0.1%, not as a cost.

The test is `crates/tsr-conformance/tests/mutable_location_return_mapper.rs`
(`new Map([[E.A, 1], [E.B, 2]])` under `Map<E, number>` prints
`Map<E.A | E.B, number>` natively).

**How this would be wrong.** Native's return mapper is a separate inference
pass over the contextual return type. If the port's
`live_contextual_return_mapper` carried inferences native's does not, a
literal would be kept where native widens. The unfiltered run moved no
line the other way.

### 3.2 `mappedTypeOverlappingStringEnumKeys` (5): a mapped template that is a conditional fails

The literal widens because the `cat` property has no contextual type, and
that is because the mapped type's member itself is `error`:

```ts
type M5 = { [V in T]: Extract<TC | SD, { type: V }> };
declare const m5: M5; const a5 = m5.cat;   // native TC, port error
type M7 = { [V in "cat" | "dog"]: Extract<TC | SD, { type: V }> };  // same
type X8 = Extract<TC | SD, { type: T.CAT }>;                         // RIGHT
```

Instantiating a mapped template that is a conditional alias over the
mapped type parameter yields `error`, even with plain string keys. That is
mapped-type instantiation (`mapped.rs`, r6-mapped), not this lane.
Routed with the probe above.

**Also found, shipped as a measured-zero diff:**
[`r6-printer-entity-name-discriminant.diff`](r6-printer-entity-name-discriminant.diff)
(main's `symbols.rs`). `discriminate_union_root` reads only string, numeric
and boolean literal initializers as discriminators.
`isPossiblyDiscriminantValue` also admits an entity-name expression, so an
enum member `E.A` discriminates. With the diff,
`const v: (TC | SD)[] = [{ type: T.CAT, address: "" }]` prints
`{ type: T.CAT; address: string; }` as native does (probe). Without it, the
§927 decline answers no contextual type and the member widens to `T`.
Measured on top of §3.1: types +0 / −0, diagnostics unchanged, slowcases
clean. No corpus line reaches the shape on its own; land it when someone
lands the mapped fix above.

### 3.3 Contextual rest-tuple labels from combined overloads: not ported, no population

Native (probed): `h((...args) => {})` against
`{ (a: string): void; (a: number): void }` prints
`(args_0: string | number) => void`, `args : [string | number]`. The port
prints `(a: string | number) => void`, `args : [a: string | number]`.
`getRestTypeAtPosition` names tuple elements by
`getNameableDeclarationAtPosition`. A combined signature's parameters are
synthetic symbols with no declaration, so the tuple has no labels. The
port's `Parameter` has no declaration, and `signature_tuple_arguments`
(main's `inference.rs`) labels every element with the parameter's name. A
faithful port needs a "has a nameable declaration" bit on `Parameter`
(signatures.rs), set off by the combiners (union_signatures.rs, main's
contextual.rs) and read in inference.rs. The rest-tuple form
(`{ (...a: [string]): void; … }`) also differs in optionality
(`args_0?: string | number | undefined` natively). No WRONG or GAP line in
the corpus prints `name_N` where the port prints a label (searched at the
base), so this is recorded, not built.

## 4. Untagged template escapes cook on the rescan (item 4, `tsr-2zk.1127`)

**Forcing constraint.** `octalLiteralAndEscapeSequence` (14 lines):
`` `\55` `` is `"-"` natively and `"\\55"` in the port. `scanEscapeSequence`
(`scanner.go:1700`) cooks a legacy octal escape only when
`ReportInvalidEscapeErrors` is set. The initial scan of a template does not
set it (`scanner.go:522`). The parser re-scans an untagged template with
reporting on (`reScanTemplateToken(false)`, `parser.go:3692`). The port's
scanner already had both arms (§147, §223). Two things stopped the cooked
value reaching the node:

1. **The parser** (main's) caches the token value on every rescan wrapper
   (`rescan_regular_expression`, `rescan_template_continuation`, …) except
   the template head. Its three call sites (`parse_primary_expression`'s
   two template arms and `parse_template_literal`) called
   `self.scanner.rescan_template` directly and kept the initial scan's raw
   value.
2. **The scanner** (this lane): `rescan_template` did not clear
   `self.value`, and `scan_template_body` seeds its buffer from it. Once the
   parser read the rescan, `` `\55` `` would have cooked to `"\\55-"`.

**Port.**

- Commit: `Scanner::rescan_template` clears the value before re-scanning,
  as `ReScanTemplateToken` resets the scan. The test is
  `crates/tsr-scanner/tests/template_rescan.rs`. Alone it is inert, because
  the parser never read the rescan value. Types +0 / −0 beyond §3.1,
  diagnostics unchanged, slowcases clean. Ir domain-model 1,090,802,692,
  generic-imports 343,084,721 (within §3.1's noise band). CLI output
  identical.
- Diff (main's parser):
  [`r6-printer-parser-rescan-template.diff`](r6-printer-parser-rescan-template.diff).
  It adds `Parser::rescan_template`, which refreshes `token_value` like the
  other rescan wrappers, and routes the three call sites through it.

**Measured, diff on top of the commit**, unfiltered against the base: types
**+14 / −0** beyond §3.1 (`octalLiteralAndEscapeSequence:0:94`–`0:100`,
`0:119`–`0:131`; the case has no non-RIGHT line left), diagnostics
unchanged, slowcases clean. Probe and whole case identical to the oracle.

## 5. Lone surrogates keep their code unit (item 5, `tsr-2zk.1128`)

Decision record:
[ADR-0051](../../adr/0051-a-lone-surrogate-is-a-plane-16-sentinel-in-a-literal-value.md),
written before the code. It lists r5-printer3 §6.2's options plus the two
sentinels.

**First build, refused by the measurement.** A one-character sentinel
(U+D800 + n stored as U+10F800 + n) measured +4 / **−2** against the base:
`unicodeExtendedEscapesIn{Strings,Templates}06(target=es6):0:1` write
`"\u{10FFFF}"`, and the sentinel printed it as `"\uDFFF"`. My pre-build
corpus check had grepped only for the raw UTF-8 bytes of plane-15/16
characters, which cannot see an escaped one. The ADR records this as the
refused option 4.

**Built:** the two-character sentinel (ADR-0051 option 5). U+10FFFF then
U+10F000 + n stands for lone surrogate U+D800 + n, and a real U+10FFFF is
itself. `Scanner::push_code_point` appends it through
`push_js_string_code_point` (`EncodeJSStringRune`), and `printing::quote`
escapes it (`escapeStringWorker`'s surrogate arm, `printer/utilities.go:84`).
A grep over both test trees finds no `\u{10FFFF}` followed by a plane-16
escape or character. Tests: `crates/tsr-scanner/tests/lone_surrogates.rs`
(round trip, `\u{10FFFF}` is not a sentinel, distinct values, pairs still
combine) and `crates/tsr-conformance/tests/lone_surrogate_literals.rs`
(native's lines for `"\uD800"`, `"\uDC00"`, a combined `\uD83D\uDE00` pair and a
lone-surrogate property name).

**Measured**, unfiltered against the base (with §3.1 and the §4 commit
underneath): types **+4 / −0** beyond §3.1
(`unicodeExtendedEscapesIn{Strings,Templates}1{0,1}(target=es6):0:1`; the
`06` cases stay RIGHT), diagnostics unchanged, slowcases clean, CLI output
identical. Ir: domain-model 1,091,023,913 and 1,092,245,024 on two runs
(base 1,090,902,888 / 1,090,897,313), inside the ±1.2M run-to-run band
§3.1 records; generic-imports 343,064,622 / 343,079,589 (base 343,079,364).
`quote` runs when a literal type is minted, so its sentinel arm matches
only the lead character U+10FFFF before decoding. A first cut that decoded
at every character measured 1,093,128,080 once (1,091,890,780 on a rerun).

## 6. Report

**Commits** (on `claude/beautiful-shannon-ar5gh0-r6-printer`, base
`b18aec06`):

| Commit | What | Types alone |
|---|---|---|
| `12d9d73` | diff only: `declare global` visibility for node reuse (§1) | — |
| `679ed32` | erased-alias reuse asks `IsSymbolAccessible` (§2.2) + node_reuse diff | +0 / −0 |
| `2d9c2ea` | mutable-location literal reads `instantiateContextualType` (§3.1) | **+5** / −0 |
| `f6c2f65` | `rescan_template` clears its value (§4) + parser diff; §3.2–§3.3 records | +0 / −0 |
| `d9a82e9` | lone surrogate sentinel, ADR-0051 (§5) | **+4** / −0 |

At `d9a82e9`: types 549,862 RIGHT (+9 against the base's 549,853),
diagnostics unchanged. The coverage run reports `checker_types`
8490/9538 (89.01%) and `diagnostics` 4636/5502 (84.26%).

**Diffs, in apply order** (each applies cleanly on `d9a82e9` after the ones
before it):

1. `r6-printer-global-augmentation-visible.diff` (node_reuse.rs) then
   `r5-declared4-print-arity-WIP.diff` (declared.rs): +37 / −0.
2. `r6-printer-serialize-type-name-accessible.diff` (node_reuse.rs): +21 / −0.
3. `r6-printer-parser-rescan-template.diff` (parser): +14 / −0.
4. `r6-printer-entity-name-discriminant.diff` (symbols.rs): +0 / −0 (§3.2).

All stacked on `d9a82e9`: types **549,934 RIGHT, +81 / −0** against the
base, diagnostics unchanged, slowcases clean on both dumps.

**Remaining, with causes:**

- `declarationEmitPartialNodeReuseTypeReferences` `c.ts` (3 lines): the site
  namer never emits an import type (`import("./a").N.SpecialString`);
  `checker.rs` (main).
- `mappedTypeOverlappingStringEnumKeys` (5): a mapped template that is a
  conditional alias instantiates to `error`; mapped.rs (r6-mapped).
- Contextual rest-tuple labels from combined overloads: no corpus
  population; needs a nameable-declaration bit across signatures.rs,
  contextual.rs and inference.rs (§3.3).
- ADR-0051's accepted gaps: `CombineSurrogatePairs` at joins, and sentinel
  decoding in the emitters. No corpus population.
