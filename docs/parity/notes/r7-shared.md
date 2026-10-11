# r7-shared — shared-contract lane (`tsr-2zk.1268`)

Round 7's shared-contract owner: `symbols.rs`, signature construction in
`signatures.rs`, `members.rs`, mapper/instantiate code, `resolution.rs` and
`crates/tsr-binder`. Two strands on separate branches: `box/r7-shared`
(independent ports, this note's §1–) and `box/r7-shared-cutover` (the
rejected alias/mapper cutover, §C). Expectations are checked against the
pinned native baselines (`vendor/typescript-go` @ `5b1047d`).

Base: `origin/main` `9020aa67`, frozen unfiltered with
`scripts/parity_gate.sh freeze`: 12,238 diagnostics rows (5,741 RIGHT, 5,610
EMPTY_RIGHT) and 556,357 type rows (551,176 RIGHT, 673 GAP, 4,508 WRONG).

## 0. Cluster re-measure on the base

r6-triage's cases (`notes/r6-triage-issues.json`, cut at `e6eadf4`) re-read
on `9020aa67`:

| Cluster | Still failing |
|---|---|
| RESOLVE-ES-MODULE-SYMBOL-CLONE | 11 of 11 (`nodeModules1`/`nodeModulesAllowJs1` ×4 modes, 24 lines each; `exportsAndImports4` ×2; `transformNestedGeneratorsWithTry`) |
| MODULE-AUGMENTATION-MERGE | 8 of 9 (`augmentExportEquals3–7`, `checkerInitializationCrash`, `mergeSymbolReexportedTypeAliasInstantiation`, `moduleAugmentationOfAlias`) |
| RESOLVE-ALIAS-INDIRECTION | 7 of 18 (batch CG landed r6-modules4 §1; the rest is r6-modules4 §4) |
| MERGE-SYMBOL-RESOLVE-ALIAS-TARGET | 4 of 6 diagnostics, 1 types |
| DEFAULT-EXPORT-ALIAS-CLASS-MERGE | 3 of 7 |
| IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET | 1 of 7 (`declFileForExportedImport`) |

## 1. `resolveESModuleSymbol`'s default-member clone (RESOLVE-ES-MODULE-SYMBOL-CLONE)

### Cause

`resolveESModuleSymbol` (`checker.go:15568`) answers a namespace import with
`cloneTypeAsModuleType(symbol, moduleType, referenceParent)` (`:15721`) when
the module type has signatures, a `default` property, or the import is an
ESM-to-CommonJS reference (`:15609-15618`). The clone is a **new symbol**:
the alias resolves to it, not to the module, so a later `import a =
require("./t1")` is the only alias that resolves to the module itself, and
the printer's `trySymbolTable` alias loop names the module `typeof a`, each
namespace import naming its own clone (`typeof c`, `typeof e2`).

For the `default`-property arm with a structured type, `moduleType` is
`getTypeWithSyntheticDefaultImportType`, which is `t` itself when the module
cannot have a synthetic default (it already has a real one). The port
returned the plain module type there, `None` from
`namespace_import_default_member_type`, so no clone existed and every alias of
the module looked alike (`exportsAndImports4`: `typeof a` for `c`/`e2`).

### The port

`namespace_import_default_member_type` now clones the module value when the
synthetic type is the value itself, exactly as the other two arms of
`module_clone_type` already do: a new anonymous `TypeId` with the same text
and symbol, recorded in `module_value_clones` as `(alias, value)` with no
signatures. This port has no clone symbol (`symbol_access.rs`'s private
symbols are not binder ids, and `resolve_alias` answers `SymbolId`), so the
clone is the alias's value type, the established stand-in
(`checker-notes-nameres.md` §14).

Checker port convention: no new cache. The existing `module_value_clones`
side table (key: the clone `TypeId`, minted once per namespace-import alias
inside `get_type_of_alias`, whose `symbol_types` memo owns it; value: the
alias and source value; published when the alias type completes; receiver
context: the alias). Member reads go through the existing clone arm of
`get_property_of_type_ex` (`members.rs`), which reads the source's members;
the work boundary is unchanged (one clone per alias, no member copy).

### Measured (symbols.rs alone)

+15 type lines, 3 cases (`exportsAndImports4(target=es2015)`,
`exportsAndImports4-es6`, `unusedImports_entireImportDeclaration`); zero
type or diagnostic losses. Coverage: `checker_types` 8,679/9,538,
`checker_types_configured` 1,753/1,928, `diagnostics` 4,786/5,502,
`diagnostics_configured` 955/1,091. Median child-CPU new/old (21 samples):
domain-model 0.9966, generic-imports 1.0077. `cargo test --workspace
--release` passes; clippy reports only the pre-existing nested fn at
`symbols.rs:4513`.

### Routed: the printer half (r7-printer, `checker.rs`)

`checker.rs::alias_targets_module_clone` (the `module_alias_at` exclusion
of an alias that resolves to a clone, `trySymbolTable`'s `resolveAlias`
comparison) admits only clones of class/function targets. Native's alias
resolves to a clone for every arm of `resolveESModuleSymbol`, so the test
should be "the alias's value is a module clone", whatever the target. With
that one change (`r7-shared-printer-module-clone.diff`) on top of §1:
**+211 type lines, 13 cases**, zero losses — the four `nodeModules1` and
four `nodeModulesAllowJs1` modes (24 lines each: `typeof m26` for `typeof
m4`, the ESM-to-CJS clone arm), `unusedImports11/12`, and §1's three.

### Remaining

`transformNestedGeneratorsWithTry` (4 lines): native's first condition is
`hasSignatures(typ)` on the type, not the target's declaration kind; an
`export = Bluebird` of a `const Bluebird: typeof Promise` is a variable with
construct signatures, which `module_clone_type` declines ("variable exports
whose value happens to be callable are a different symbol shape"). Needs the
synthetic spread of `PromiseConstructor` plus `default`, printed in the
spread's member order.

## 2. `mergeSymbol` through an alias target (MERGE-SYMBOL-RESOLVE-ALIAS-TARGET, `.38`)

### Cause

`mergeModuleAugmentation` (`checker.go:1397`) runs `mergeSymbol(mainModule,
augmentation.Symbol)`, whose `mergeSymbolTable` recursion meets the target
module's export entries. An entry that is an **alias** (`export default I`,
`export type { Row } from "./common"`) is not merged as an alias:
`mergeSymbol` resolves the non-transient target (`resolveSymbol`,
`:14153`) and either merges the source into a clone of the resolved symbol,
or — the source's excludes hitting the resolved flags — reports
`reportMergeSymbolError(target, source)` and **returns `source`**.
`mergeSymbolTable` stores the return value (`target[id] = merged`,
`:14117`), so the conflicting augmentation declaration displaces the alias
in the module's exports. The binder declined every alias-target merge
(`alias_merges`, "nothing here follows aliases", `bd tsr-y4u.12`); only the
checker's TS2300/TS2451 report ran.

Second, `mergeSymbolTable` inserts `getMergedSymbol(sourceSymbol)` for a name
the target lacks. The binder inserted the raw source, so an augmentation's
`interface EventList` merged through `export *` into the re-exported
declaration (`:1433-1441`) entered `index`'s exports unmerged, and a second
augmentation of `./index` merged into that stale symbol instead of the one
`./eventList` exports.

### The port (`crates/tsr-binder`)

- `BindResult::merge_through_alias_targets`: after each augmentation's
  `merge_pairs`, every alias merge it declined is re-decided as upstream
  does. `resolve_alias_for_merge` is `resolveSymbol` over the forms this
  binder can follow with its own name resolution and the program's module
  resolution callback — `export default X`/`export = X` of an identifier,
  local and re-exported export specifiers, import specifiers and default
  imports of modules without `export =` — to the first non-alias symbol,
  merged, bounded (16 hops, cycle check). A conflict stores the source in the
  alias's slot and keeps the pair for `Checker::report_merge_conflicts`
  (unchanged, so TS2300/TS2451 still land on the alias's declarations); a
  merge unions the source into the resolved symbol in place (the binder's
  in-place merge stands for `cloneSymbol`, per `merge_symbol`'s doc), stores
  it in the slot and drops the pair.
- `Binder::merge_symbol` inserts `merged_symbol(source)` for a name the
  target lacks, in both `members` and `exports`.

Rejected: resolving through the checker. `merge_module_augmentations` runs
before any checker exists and the checker holds the `BindResult` immutably;
deferring the merge to the checker would make member tables checker-relative,
which is the shared-across-checkers state ADR-0003's side tables avoid.
Accepted limitation: an alias the binder cannot follow (an `export =`
member, a qualified import-equals, a namespace import) keeps the decline,
and upstream's `unknownSymbol` arm (an unresolvable alias takes the source)
is not taken, because "cannot follow here" is not "resolves to nothing".
`mergedParent` (`:14118`, re-parenting the merged symbol to the augmented
module) is not ported: in-place merging would re-parent the original
declaration, which upstream's clone never does.

Checker port convention: no cache or side table; the binder's existing
`merged` redirect map is the publication (written once per merge, before
the program is shared). The work boundary is one bounded syntactic alias
walk per declined augmentation merge.

Falsifier: a case where an augmentation merges into an alias whose target
this walk resolves differently from the checker's `resolve_alias_fully`
(the two must agree; a TS2300 on a pair the binder merged would show it).

### Measured (against §1's freeze)

+4 type lines, +4 diagnostics cases, zero losses:
`moduleAugmentationOfAlias` (diagnostics and types),
`mergeSymbolReexportedTypeAliasInstantiation` (diagnostics and types),
`mergeSymbolRexportFunction`, `mergeMultipleInterfacesReexported`.
Coverage: `checker_types` 8,681/9,538, `checker_types_configured`
1,753/1,928, `diagnostics` 4,788/5,502, `diagnostics_configured` 955/1,091.
Median child-CPU new/old (21 samples, against §1's binary): domain-model
1.0259, generic-imports 1.0117. Tests pass; clippy clean in touched code
(`tests/jsdoc_import_duplicates.rs`'s `implicit_clone` is pre-existing).

### Remaining in the augmentation clusters

- `augmentExportEquals3/4/6` and `augmentExportEquals5`'s `typeof e`
  lines: printer. A symbol with a non-global augmentation declaration is
  printed as an import type (`symbolToTypeNode`'s
  `hasNonGlobalAugmentationExternalModuleSymbol` arm): `typeof
  import("./file1")`. The merge already adds the declaration; routed to
  r7-printer.
- `augmentExportEquals5` `x.id` (3 lines), TS2454: §3 (held).
- `augmentExportEquals7`: `import * as lib` of an `export =` of a
  `var`+`namespace`: the synthetic default wrapper prints `{ default: () =>
  void; }`; `module_clone_type` / `getTypeWithSyntheticDefaultOnly` arm.
- `checkerInitializationCrash`: a `declare global` alias merge (`export
  import VNode = react.ReactNode` against `type VNode`) needs the
  qualified import-equals through a UMD `export =`, which the binder walk
  declines; TS2300 ×2 and `VNode` missing.

## 3. HELD: `resolveESModuleSymbol`'s pure-alias step in `getExternalModuleMember`

### Cause

`getExternalModuleMember` (`checker.go:14667`) reads members off
`resolveESModuleSymbol(moduleSymbol)`, whose second step (`:15570`) resolves
an `export =` symbol that is a pure alias (`export = e`, `IsNonLocalAlias`)
through `resolveIndirectionAlias`. `get_external_module_member` stopped at
`resolveExternalModuleSymbol(…, dontResolveAlias = true)`, so for `export =
e` it read the exports of the `export=` alias symbol itself (empty) and the
module's own table (only `export=`): `import { Request } from "express"`
resolved to nothing although the augmentation had merged `Request` into
`e` (`augmentExportEquals5`, `x.id : any`, TS2454 missing).

### Measured, and why it is held

`r7-shared-HELD-export-equals-pure-alias.diff`, against batch-2 main
`1b466dc8`: +3 type lines and +1 diagnostics case (`augmentExportEquals5`),
**−8 type lines** — refused. The losses are a printer gap the resolution
exposes, not a wrong target: `contextuallyTypedJsxAttribute2` ×7 and
`declarationEmitExportAssignedNamespaceNoTripleSlashTypesReference` 2:2
import `ElementType` / `Component` by name from react's `export = React`.
Unresolved, the reference printed its written text (`ElementType`,
`Component`), which matched native by accident; resolved, TSR prints the
reference with its filled default arguments (`ElementType<any>`,
`Component<any, {}, {}>`), where native's node builder omits type arguments
that equal the declared defaults for a reference written without them.
Re-land when the printer drops default-equal trailing arguments (routed,
r7-printer); the diff then needs re-measuring.

## 4. `resolveQualifiedName`'s left side through an alias (QUALIFIED-NAME-LEFT-ALIAS-RESOLVE)

### Cause

`import EnumA = Enum.A` after `import { Enum } from "./enum"`:
`getSymbolOfPartOfRightHandSideOfImportEquals` → `resolveEntityName` →
`resolveQualifiedName` (`checker.go:15828`) resolves the left side with
`resolveEntityName(left, Namespace, …, dontResolveAlias = false)`, whose
`resolveSymbol` follows the pure import alias to the enum, then reads `A`
from its exports; a merged alias whose own exports lack the name falls back
to its target's (`:15852`). `resolve_qualified_entity` read `exports` off
the alias symbol as found and answered `None`, so `EnumA` (merged with an
exported type alias in `importedEnumMemberMergedWithExportedAliasIsError`)
read `any`; r7-printer had kept an older same-file arm to cover that line.

### The port

Each left segment is merged, a pure alias (`is_non_local_pure_alias`) is
resolved through `resolve_alias`, and a missing name on a merged alias falls
back to the alias target's exports. No cache: `resolve_alias`'s existing
memo is the only state touched.

### Measured (against batch-4 main `660718af`)

+9 type lines, zero losses, diagnostics unchanged; 2 cases:
`importedEnumMemberMergedWithExportedAliasIsError` (the routed 1:1 line)
and `declarationEmitEnumReferenceViaImportEquals` (7 lines);
`leaveOptionalParameterAsWritten` 1:6 also turns right. One line moves
GAP→WRONG: `leaveOptionalParameterAsWritten` 2:5, where `export import Foo
= a.Foo` inside `declare global { namespace teams.calling }` now resolves as
native resolves it, and the printer spells the reference
`teams.calling.Foo | undefined` where native's accessible-chain search
answers `import("./a").Foo | undefined` (routed, r7-printer).
Coverage: `checker_types` 8,738/9,538, `checker_types_configured`
1,771/1,928, `diagnostics` 4,849/5,502, `diagnostics_configured` 968/1,091.
Median child-CPU new/old (21 samples): domain-model 1.0009,
generic-imports 1.0108. Workspace clippy (`--all-targets -D warnings`) clean;
`cargo test --workspace --release` passes.

## 5. ROUTED: native `resolveSymbol` for the symbol-chain comparisons (RESOLVE-ALIAS-INDIRECTION remainder)

### Cause

`resolveSymbolEx` (`checker.go:16258`) follows an alias only when it is pure
(`IsNonLocalAlias`); a symbol merged with another meaning (`import * as B`
merged with `interface B` and re-exported, `noCrashOnImportShadowing`) is its
own answer. `resolve_alias_fully` walks on through such merged symbols to the
end of the chain. In the printer's `trySymbolTable` comparisons
(`module_alias_at` and its siblings in `checker.rs`) that made `import { B }
from "./a"` look like an alias of module `./b`, so the module printed `typeof
B` where native, finding `B` resolves to a.ts's merged symbol, names it
through `import * as OriginalB` (`typeof OriginalB`).

### Measured and why it is not one global change

Making `resolve_alias_fully` itself stop at merged symbols: +11 type lines,
**3 losses** (`mergedWithLocalValue` 1:0, `shadowedInternalModule` 0:36,
`verbatimModuleSyntaxNoElisionCJS` 5:3). Those reach
`get_declared_type_of_alias`, which is native `getDeclaredTypeOfAlias` —
`getDeclaredTypeOfSymbol(resolveAlias(symbol))`, recursing through a merged
alias — so the walking form is the native one there. The 34 callers mirror
different native operations and have to be switched one by one.

`r7-shared-printer-resolve-symbol.diff` adds `Checker::resolve_symbol`
(symbols.rs, native `resolveSymbol`) and switches the seven `checker.rs`
symbol-chain call sites to it, keeping `get_declared_type_of_alias` on
`resolve_alias_fully`. Against `f11e37b8`'s freeze: **+11 type lines, 0
lost**, diagnostics unchanged; cases `noCrashOnImportShadowing` and
`typeAndNamespaceExportMerge`, plus five lines of
`exportTypeMergedWithExportStarAsNamespace`. `cargo clippy -p tsr-checker
--all-targets -D warnings` is clean with it applied. It cannot land from this
lane: the call sites are r7-printer's, and `resolve_symbol` alone would be
dead code under the clippy gate. Routed whole to the integrator.

Remaining `resolve_alias_fully` callers to audit against their native
operation (resolveSymbol vs resolveAlias vs getSymbolFlags' chain walk):
declared.rs ×3, flow.rs ×2, jsx_*.rs ×3, node_reuse.rs ×2,
import_type_meaning.rs ×2, symbols.rs ×6, one each in emit_helpers,
enum_initializer, members, meaning_mismatch, merge_conflicts, printing,
symbol_accessibility.

## 6. Identifier import-equals through an exported alias (IMPORT-EQUALS-IDENTIFIER-ALIAS-TARGET)

### Cause

`export import a = require("./m"); export import b = a;`: native
`getTargetOfImportEqualsDeclaration` → `getSymbolOfPartOfRightHandSideOfImportEquals`
→ `resolveEntityName(a, Namespace)`, whose `getSymbol` on the module's
exports admits the exported alias `a` by its target's meaning
(`checker.go:2176`). An exported import-equals has no local symbol, so the
binder's plain `resolve_name` (alias callback `Some(false)`, "continue
outward") never answered it: `resolve_alias(b)` was `None`, and
`get_type_of_alias`'s §144 pre-check ("an entity root that resolves to
nothing reads error-any") read the same miss as an unresolvable root and
answered `any` (`declFileForExportedImport`, 5 lines).

### The port

The identifier arm of `get_target_of_alias_symbol` uses
`resolve_name_with_export_alias`, the export-alias-aware walk
`export_specifier_target` already uses (r6-modules4 §1.4). The §144 root
pre-check fires only when both the binder walk it used and that walk find
nothing: an exported alias now counts as a resolving root, and every root
the binder walk found keeps its classification. Making the pre-check the
export-alias walk alone also reclassified `symbol_chain.rs`'s function-only
`import Linked = Head` chain (a local alias whose target lacks the namespace
meaning, native TS2503) from the gap to §144's `any` — on some fixtures
only, depending on whether the outer resolution frame pops cleanly; that
reclassification is left for a separate, explained change. No cache; the
callback reads `resolve_alias`'s memo.

### Measured (against `f11e37b8`)

+5 type lines, zero losses, diagnostics unchanged: `declFileForExportedImport`.
Coverage: `checker_types` 8,739/9,538, `checker_types_configured`
1,771/1,928, `diagnostics` 4,849/5,502, `diagnostics_configured` 968/1,091.
Median child-CPU new/old (21 samples): domain-model 1.0058,
generic-imports 0.9953. Workspace clippy clean; tests pass.

## 7. `resolveESModuleSymbol`'s `hasSignatures` arm for a callable variable `export =`

### Cause

`resolveESModuleSymbol` (`checker.go:15610`) clones a namespace import when
`hasSignatures(typ)` — a property of the module's *type*, whatever declares
it. `module_clone_type` dispatched on the target's declaration kind: class →
construct signatures, function → call signatures, an ESM-to-CJS module, and
everything else to `namespace_import_default_member_type`, which admitted
only a type with a `default` property. `declare module "bluebird" { const
Bluebird: typeof Promise; export = Bluebird }` is a variable with construct
signatures, so `import * as Bluebird` kept `PromiseConstructor` itself where
native answers the synthetic-default spread with no signatures, `{ all…;
[Symbol.species]: PromiseConstructor; default: PromiseConstructor; }`.

### The port

`namespace_import_default_member_type`'s gate is native's: call or construct
signatures, or a `default` property. The rest of the arm is unchanged
(`getTypeWithSyntheticDefaultImportType`, or §1's clone of the unchanged
type, which carries no signatures). A synthetic type this port answers as
the gap — the spread of a `const Foo = class …` constructor value's static
side — declines to the uncloned value, as before, rather than introduce a gap
(`symbol_chain.rs`' `module_class_clone_boundary_does_not_admit_other_export_meanings`
pins that boundary; natively that import is cloned too). No new cache.

`symbol_chain.rs`' `const Foo = function …; export = Foo` fixture pinned the
old boundary (`Head` equal to `Raw`'s function type); natively the namespace
import is the signature-less synthetic-default object, which it now asserts.

### Measured (against `2fd5c9f4`)

+3 type lines, zero losses, diagnostics unchanged:
`transformNestedGeneratorsWithTry` 0:0/0:8 and `augmentExportEquals7` 1:0
(`{ default: () => void; }`, the same arm for a callable `var` `export =`),
which converts that case. Coverage: `checker_types` 8,801/9,538,
`checker_types_configured` 1,787/1,928, `diagnostics` 4,896/5,502,
`diagnostics_configured` 981/1,091. Median child-CPU new/old: a 21-sample
run read domain-model 1.0633 / 0.9252 on two runs; the 41-sample rerun is
domain-model 0.9810, generic-imports 0.9701. Workspace clippy clean; tests
pass. Remaining in `transformNestedGeneratorsWithTry`: `b()` / `await b()`
read `any` (a call of an async function annotated `Bluebird<void>`), a
separate root.

## C. The rejected alias/mapper cutover (strand 2, `tsr-2zk.16.56.1.4`)

### What the branch is

`origin/recovery/rejected-alias-shared-20261007` is 41 commits on
`db726c9c`, which was 1,581 commits behind round 7's base. None of the 41
exists on main by patch-id or subject. The last two (`42779636`,
`f7faed0c`) are a 55-file / +3,530-line WIP snapshot (SymbolRef require
symbol, pending-callable displays, `find_cycle_start`, alias links, a
singular `instantiation_expression.rs` beside main's
`instantiation_expressions.rs`); the 39 before them are calls, inference,
parser and property-view commits. A squash merge onto main conflicts in 27
files (~90 hunks); the first commit already conflicts semantically (main
caches interface signatures by `(SymbolRef, kind)`, the cutover threads a
`this_argument` that would have to join that key). The recovery head does
not build every example (`infergen`), and its `diagverdictdump`
exhausts the orb's 7 GB (OOM-killed twice with the box otherwise idle).

### Measured in its own era (types; diagnostics could not run)

Unfiltered `verdictdump`, `db726c9c` vs the recovery head: **+587 / −94**
RIGHT type lines (base 469,826 RIGHT of 477,970; head 470,319 of 478,047).
Against current main (`f51103ba`, batch 13):

- **410 of the 587 gains are already RIGHT on main**, landed by other lanes'
  ports since; **177 remain**, concentrated in about 40 cases — the largest:
  `expressionWithJSDocTypeArguments` 34, `controlFlowAliasing` 16,
  `emptyObjectNotSubtypeOfIndexSignatureContainingObject1/2` 25,
  `chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2` 16,
  `inferenceContextualReturnTypeUnion2` 6, `keyofIntersection` 5,
  `selfReferentialFunctionType` 5, `emptyTypeArgumentList(WithNew)` 10.
- **All 94 of its losses are RIGHT on main**, so each would be a main loss
  (`ramdaToolsNoInfinite2` 12, `reverseMappedTypeInferenceWidening1` 8,
  `recursiveTypeReferences1` 5, `genericRestParameters1` 5, …).

Of the cutover's 11 new test files run on main: 2 pass, 5 fail (concrete
`this` in inherited call signatures, construct-only types not callable,
native rest arity, instantiation-expression aliases and views), 4 do not
compile (`skip_trivia`, `type_argument_list_span`,
`InstantiationExpressionSignature` absent).

### Decision

Rebasing the line would buy at most 177 lines while carrying 94 repairs, an
OOM and 90 conflict hunks against code main has since rewritten. The
alternative taken: treat the 177 still-missing lines and the five failing
tests as a work list, port each behavior directly on main in its owning
lane, and keep the branch as the record it is. That changes the strand from
"rebase and repair" to "harvest"; it is the integrator's call
(`docs/parity/round7.md`). What would make the rebase win: a later main
where the 94 losses' cases are already covered by tests that would catch
them, and the conflict count dropping below a few hunks.

Still-missing gain cases (lines): compiler/expressionWithJSDocTypeArguments 34; conformance/controlFlowAliasing 16; compiler/emptyObjectNotSubtypeOfIndexSignatureContainingObject2 16; compiler/chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2 16; compiler/emptyObjectNotSubtypeOfIndexSignatureContainingObject1 9; compiler/inferenceContextualReturnTypeUnion2 6; conformance/keyofIntersection 5; compiler/selfReferentialFunctionType 5; compiler/emptyTypeArgumentListWithNew 5; compiler/emptyTypeArgumentList 5; conformance/genericRestParameters1 4; conformance/strictBindCallApply1 3; conformance/importCallExpressionWithTypeArgument 3; conformance/conditionalTypes1 3; compiler/selfReferentialDefaultNoStackOverflow 3; compiler/ramdaToolsNoInfinite 3; compiler/circularInstantiationExpression 3; compiler/aliasInstantiationExpressionGenericIntersectionNoCrash1 3; conformance/mappedTypeRelationships 2; conformance/arbitraryModuleNamespaceIdentifiers_importEmpty 2; compiler/promiseChaining2 2; compiler/promiseChaining1 2; compiler/promiseChaining 2; compiler/missingCommaInTemplateStringsArray 2; compiler/excessPropertyCheckIntersectionWithRecursiveType 2; compiler/enumAssignmentCompat4 2; compiler/deferredConditionalTypes 2; compiler/arrayFlatNoCrashInferenceDeclarations 2; compiler/arrayFlatNoCrashInference 2; compiler/aliasOfGenericFunctionWithRestBehavedSameAsUnaliased 2; conformance/thisTypeErrors 1; conformance/readonlyArraysAndTuples 1; conformance/genericRestParameters2 1; compiler/unusedTypeParametersNotCheckedByNoUnusedLocals 1; compiler/unusedTypeParametersCheckedByNoUnusedParameters 1; compiler/ramdaToolsNoInfinite2 1; compiler/parserPrivateIdentifierInArrayAssignment 1; compiler/keyRemappingKeyofResult 1; compiler/inferredRestTypeFixedOnce 1; compiler/conditionalTypeGenericInSignatureTypeParameterConstraint 1; compiler/circularlyReferentialInterfaceAccessNoCrash 1; 

## 8. `getExternalModuleMember` admits every `export =` member (EXTERNAL-MODULE-MEMBER-EXPORT-EQUALS)

### Cause

`getExternalModuleMember` (`checker.go:14667`) reads a named import from an
`export =` module as `getPropertyOfType(getTypeOfSymbol(target), name,
skipObjectFunctionPropertyAugment = true)`, with no condition on the
member's type. The port admitted only scalar-typed members (string, number,
literal, enum-like, unique symbol, …): an object or function member "can
carry named types whose shortest accessible chain depends on this import
site", so it was withheld to keep the printer from spelling it wrongly.
`import { f } from "./m"` of `export = { f }`-style namespaces, classes and
`EventEmitter` read `any`, and `import * as pick` merged with a local
`const pick` lost TS2440/TS7023's inputs.

### The port

The type gate is removed; alias members still decline (the branch does not
`resolveSymbol` them). No cache.

### Measured (against `f51103ba`)

+20 type lines, +1 diagnostics case, zero losses:
`aliasDoesNotDuplicateSignatures` converts (types and diagnostics);
`importDeclFromTypeNodeInJsSource` (10 lines), `conflictingDeclarationsImportFromNamespace1/2`
(3 each) and `namedImportNonExistentName` (2) gain lines. Three lines move
GAP→WRONG, all `exportAssignedNamespaceIsVisibleInDeclarationEmit` 1:0–1:3's
spelling: the member now resolves to native's type and the printer writes
`Foo.Bar` where native's accessible-chain search answers
`import("./thing").Bar` (routed to r7-printer with §4's similar line). That
is the trade the old gate refused; at 20 right for 3 wrong spellings, with
no right line lost, it is taken.
Two `cross_file_aliases.rs` tests pinned the retired gate and now pin the
native answer: `make` from `export = Owner` resolves (its spelling is the
printer's), and `Item` resolves to the target's object member over the
module's invalid numeric export. Re-verified on batch-14 main `9db14f99`
together with §7 (+23 type lines, +1 diagnostics case, 0 lost); coverage
`checker_types` 8,814/9,538, `checker_types_configured` 1,796/1,928,
`diagnostics` 4,928/5,502, `diagnostics_configured` 1,003/1,091; child CPU
21 samples 1.0073 / 0.9207; jsTyping new_false 0, lost_true 0; callgrind Ir
domain-model 0.99996, generic-imports 0.99987; strict clippy and tests green.

## 10. A JS require binding element binds as an alias (`IsVariableDeclarationInitializedToRequire`)

### Cause

`bindVariableDeclarationOrBindingElement` (binder.go:1168) binds a
declaration as an `Alias` when `IsVariableDeclarationInitializedToRequire`
holds, and that predicate climbs a binding element's `Parent.Parent` to its
variable declaration (utilities.go:2825): in `const { K } =
require("./mod1")`, `K` is an alias, resolved by
`getTargetOfImportSpecifier`'s `IsBindingElement` arm through
`getExternalModuleMember(GetRootDeclaration(node), node)`, whose module
specifier is the require call's argument (`getExternalModuleRequireArgument`).
The binder admitted only an identifier-named variable declaration, so `K`
was a block-scoped variable typed by destructuring.

### The port

- Binder: `is_require_alias_variable` admits a binding element with an
  identifier name whose pattern's parent is a require declaration
  (`is_require_declaration`, the old variable test); one level only, as
  native.
- `symbols.rs`: `declaration_of_alias_symbol` admits such a binding element;
  `get_target_of_alias_symbol` routes it to `get_external_module_member`
  with the declaration as root; that function takes the specifier from the
  require call (`require_declaration_argument`) and the name from the
  element's property name or name.
- `declared.rs` (r7-declared's file, granted/routed): the type-reference
  alias road admits an unrenamed binding element exactly as it admits an
  unrenamed import specifier; without it `@param {K} k` lost three right
  lines (`commonJSImportClassTypeReference` 0:4/0:5/0:8).

No cache.

## 11. Implied binding-pattern types for type-position parameters (BINDING-PATTERN-IMPLIED-TYPE)

`getTypeForVariableLikeDeclaration`'s binding-pattern arm
(`getTypeFromBindingPattern`, checker.go:17904) types an unannotated pattern
parameter by its pattern wherever no contextual type applies.
`get_widened_type_for_variable_like_declaration`'s §429 gate admitted
function and method declarations, and arrows/function expressions only where
`has_no_contextual_type` shows there is none; a function type's or
signature member's parameter fell through to the gap. Those parameters sit
in a type node, which `getContextualType` cannot reach, so they are
admitted unconditionally — the list `signatures.rs::parameter_of`'s §875
already uses: `FunctionType`, `ConstructorType`, `CallSignature`,
`ConstructSignature`, `MethodSignature`, `IndexSignature`. No cache.

## 12. A conflicting global merge over a UMD alias stores the source (checkMergedGlobalUMDSymbol)

`initializeChecker` puts `export as namespace THREE`'s alias in the globals
first-in-wins, then merges `declare global { export const THREE }` into it.
`mergeSymbol` resolves the alias (to the `./global` module, a value module),
the const's excludes hit it, and the conflict arm reports
`reportMergeSymbolError` and returns `source`, which `mergeSymbolTable`
stores: `globals["THREE"]` is the const, so `const m = THREE` in another
file is `typeof import("./three")`. The binder's UMD arm handled only the
non-conflicting merge; `BindResult::umd_alias_conflicts` is the other half —
the slot takes the source and the pair stays in `alias_merges`, so the two
TS2451 reports are unchanged. No cache.

## 13. A whole-type `getTypeWithThisArgument` (step 1 built; steps 2–3 planned)

Routed from r7-grammar: TS2415/TS2430 false positives (`contextualThisType`,
`thisTypeInFunctions3`, `genericDefaults`, `indexSignatureInOtherFile/1`)
because `relate_ternary` compares declared types whose `this` differ. Also
the harvest spec `calls_concrete_receiver.rs` (inherited `(value: T): this`
called through `Derived` returns `Derived`). Nothing below exists yet.

**Native operation.** `getTypeWithThisArgument(t, thisArgument)`
(`checker.go:19573`): a class/interface reference `C<A…>` becomes
`createTypeReference(C, [A…, thisArgument])` — the this-type is one more type
argument, so `C<A>` with `this = D` is a distinct reference with its own
identity, members and relations; an intersection maps per constituent.
Callers: `resolveObjectTypeMembers` (each base is instantiated with the
derived type's `thisType`), `getBaseTypes` consumers in
`checkClassLikeDeclaration`/`checkInterfaceDeclaration` (TS2415/TS2430
relate the derived type to `getTypeWithThisArgument(base, derived.thisType)`),
and member access (`getTypeOfPropertyOfType` on the receiver with the
original receiver as this).

**Today.** `this_types[class symbol]` mints one this-type per class
(`expressions.rs`); `members.rs` §164/§165 substitutes it per member read
(`get_type_of_property_with_this_argument`); `calls.rs` substitutes a
returned this-type at a property-access call. There is no reference whose
identity carries a this-argument, so relations and inherited signatures see
the base's own `this`.

**Proposed identity and owner.** Keep `type_reference_targets`' `(SymbolId,
Vec<TypeId>)` as is (225 readers). Add one Checker-owned intern table
`this_argument_references: FxHashMap<(TypeId /*reference*/, TypeId /*this*/),
TypeId>` and its inverse `reference_this_arguments: FxHashMap<TypeId, (TypeId
/*reference*/, TypeId /*this*/)>`. The image is a new `TypeId` with the same
`type_reference_targets` entry (same target and arguments) — exactly native's
"same target, one more argument" — so every existing reader sees the
reference it expects, and only this-aware readers consult the inverse table.
A reference with no entry has itself as its this-argument (native's default
`thisArgument = target.thisType`, which the reference's mapper sends to the
reference).

**Publication.** Interned on first request, before any member of the image
is read; never removed; no provisional state (the image is a pure function
of its two `TypeId`s).

**Receiver context.** Member reads on an image take its this-argument
(`members.rs`' §164/§165 read becomes "this-argument of the receiver, else
the receiver"); `signature_candidates_of_interface_symbol` instantiates each
inherited base through `get_type_with_this_argument(base, derived this-type)`
before the receiver mapper — the `interface_signatures` key then needs no
this component, because the declared set is expressed in the derived's own
this-type and the receiver mapper substitutes it; the relater compares an
image structurally through those members.

**Expensive work.** Bounded by the images requested: one per (base
reference, derived this-type) from base resolution and one per heritage
check; member images reuse the existing per-reference member caches keyed by
the image `TypeId`.

**Order.** (1) the intern tables and `get_type_with_this_argument` in
`members.rs`/`instantiate` (this lane), with unit tests from the harvest spec;
(2) inherited signatures (`signatures.rs`, this lane); (3) the heritage
checks' consumer (`heritage_conformance.rs`, r7-grammar) and the relater's
structural arm (`relater.rs`, r7-reports), coordinated through the
integrator. Each step gated alone. Falsifier: a corpus case where two
references differing only in this-argument must be identical natively (none
known — native's identity is by argument list).

## 14. `getLateBoundSymbol`'s declarations as a query (late-bound member symbols)

Routed from r7-reports: TS7032/TS7006 on a late-bound accessor pair
(`get [k]()` / `set [k](v)` with `const k = "m"`) decline because
`implicit_any.rs::accessor_pair` reads the member's own `__computed` symbol,
which never holds the sibling accessor; native reads the late-bound symbol
(`getSymbolOfDeclaration` → `getLateBoundSymbol`, checker.go:14390), whose
declarations `lateBindMember` (:16005) gathered by property name.

The binder keeps one anonymous `__computed` symbol per computed member and
the checker already resolves the late-bound names
(`members.rs::late_bound_members_of`, memoised in `late_bound_member_names`);
`get_type_of_accessors_worker` reconstructed the pair inline (§523).
`Checker::late_bound_symbol_declarations(symbol)` is that reconstruction as
a query: the declarations of the container and staticness whose computed
names bind to the same late-bound name, in declaration order. The accessor
typing now calls it (behavior-neutral); `implicit_any.rs`' accessor pair is
its second consumer (r7-reports' file, routed): for a `__computed` own
symbol, take the getter/setter from these declarations and drop
`accessor_has_late_bindable_name`'s decline.

Not built: a late-bound *symbol* identity (native's merged transient symbol
with its own links). Every reader here goes by declarations; a consumer that
needs one symbol for the pair (relations of a late-bound property, its
flags) still reads the per-member `__computed` symbol.

Checker port convention: no new cache; the publication is the existing
`late_bound_member_names` memo (key `(container SymbolId, is_static)`, owned
by the Checker, an empty sentinel while resolving), and the expensive work
stays one name resolution per computed member per container.

### Step 1 (built): the identity, no consumer

Approved by the integrator in outline (keep `type_reference_targets`' key;
interned `(reference, this) → image` and its inverse; the image shares the
reference's target and arguments). `Checker::get_type_with_this_argument`
(`members.rs`) and the two tables (`checker.rs`) exist, with the
conventions entry on the function. No caller reads an image yet, so the
block carries `#[allow(dead_code)]` until step 2. Unit test
`this_argument_tests`: an image is interned once, differs from its
reference, shares its `type_reference_targets` entry and printed text; a
reference's own this-argument is the reference; `None` takes the target's
this-type; a non-reference is unchanged. Expected corpus delta: zero.

### Step 2 (built, HELD on `box/r7-shared-held`): inherited and receiver signatures take the this-argument

- `signatures.rs::instantiate_signature_for_receiver`: the existing reference
  mapper (`instantiate_signature_for_reference`, `inference.rs`, unchanged)
  followed by the target's this-type(s) → the receiver's this-argument (an
  image's, else the receiver itself), used for a named receiver's
  call/construct candidates; inherited bases are instantiated through
  `get_type_with_this_argument(base, derived declaration's this-type)`.
- This-type identities: a class has one (`this_types`), an interface one per
  *declaration* (`this_type_nodes`, `declared.rs`), unlike native's one per
  symbol, so the substitution maps every minted one (unifying them is
  `declared.rs`', r7-declared).
- Harvest spec `tests/calls_concrete_receiver.rs` passes with it.
- Step 2b: `get_type_of_property_of_type` passes an image's this-argument.

**Why held.** Corpus delta zero (batch 19, both steps), but callgrind Ir
domain-model **+0.57%** (step 2) and +0.08% (2b): every named-type
candidate set now takes a second instantiation pass whenever the target has
a minted this-type, and lib interfaces (`Array`'s `this is S[]` predicates)
do. Native pays this inside one cached member resolution per reference.
Before re-landing with step 3: fold the this pairs into the reference
mapper's single pass and skip targets whose this-type the signature cannot
mention, then re-measure Ir.

## 15. `isReadonlySymbol` of a mapped member in `is_readonly_property_of_type`

Routed from r7-flow (holds its r6-errorsplit3 diff R: +16
`controlFlowAliasing`, −2 `mappedTypes6` 277/279). `isReadonlySymbol(
getPropertyOfType(receiver, name))` (checker.go:13849) reads, for a mapped
member, the transient mapped symbol whose `CheckFlagsReadonly`
`resolveMappedTypeMembers` (:20894) set from the mapping's `readonly` /
`-readonly` modifier or the modifiers property. This port's
`get_property_of_type` answers the modifiers type's own symbol, so the
non-union tail read `Bar.b`'s `readonly` for `Readwrite<Bar>`'s `b`.
The tail now asks `readonly_target.rs::constituent_property_is_readonly` —
the rule that file already applies to a union constituent (homomorphic
`mapped_identity_optionality` first, then the member image's modifier, then
the symbol, `property_signature_is_readonly` included as native's
`isReadonlySymbol` covers a readonly property signature). The only change in
r7-flow's file is that function's visibility (`pub(crate)`). No cache.

## 16. `bindNamespaceExportDeclaration`'s placement diagnostics

Routed from r7-grammar. `export as namespace N` (binder.go:820): any modifier
is TS1184 and the declaration still binds; then outside a source file's top
level TS1316, in a script TS1314, in a non-declaration file TS1315 — each on
the statement, and none of the three declares the UMD alias. The binder
declared it unconditionally and reported nothing. `Binder::statement_span`
is `getErrorSpanForNode`'s default arm (the node's span from its first token).

Not ported here: TS18012 (`#constructor`, `checkPrivateIdentifier`,
binder.go:1323) is reported only when the file has no parse diagnostics
(`len(b.file.Diagnostics()) == 0`), which this binder cannot see — its
inputs are the AST and `FileInfo { name, text }`. Porting it faithfully needs
the parse-diagnostic count at bind time (a `FileInfo` field, which every
construction site across the compiler, conformance and test crates would
have to supply), routed back to the integrator.

Perf: the arm sits behind `declare`'s `GlobalExports` destination (unique to
this declaration) in a `#[cold]` function, so other nodes pay nothing. The
binder's growth from this lane's arms pushed four tiny predicates
(`SymbolFlags::excludes`, `computed_property_name`,
`is_exported_from_container`, `is_parameter_property`) out of line in the
merged `bind_inner`: callgrind Ir generic-imports +0.42% until they were
marked `#[inline(always)]` (`#[inline]` alone was not honoured), after which
the lane's head reads generic-imports 1.0011, domain-model 1.0003 against
batch 19.
## 17. Bind-time alias merges and import-equals / namespace-import targets

§2's alias-target pass ran only over the merges an augmentation declined.
`initializeChecker` merges script locals and `declare global` blocks into the
globals before any module augmentation, through the same `mergeSymbol`, so
the bind-time declines (`merge_into_globals`) are re-decided first, from
index 0 of `alias_merges`. `resolve_alias_for_merge` gains
`getTargetOfImportEqualsDeclaration`'s three forms (a `require`, through
`export =`; an identifier; a qualified name, each left side resolved as
`resolveQualifiedName` does) and `getTargetOfNamespaceImport` (the module
through its `export =`). A local import alias is found by asking the binder's
lookup for `NAMESPACE | ALIAS`: its meaning filter only admits an alias by
the target's meaning through the checker's callback, which the binder does
not have, so the walk finds the alias itself and follows it.

`checkerInitializationCrash`: `export import VNode = react.ReactNode` in a
global namespace meets `type VNode = preact.VNode<any>`; the alias resolves
to `React.ReactNode` (a type alias), the excludes hit, so the namespace's
`VNode` slot takes the source and both TS2300s report. Not converted by this
alone: the alias side's TS2300 sits at the statement where native names the
alias (`GetNameOfDeclaration(ImportEqualsDeclaration)`, the name;
`merge_conflicts.rs::report_merge_symbol_error` has that arm only for
`export as namespace`), and line 1:2 (`type VNode = preact.VNode<any>` reads
`any`) is the declared-type-alias body road over a qualified name through an
alias — the root routed to r7-declared with `conformance/enums`.

## 18. A circular type-parameter constraint is no constraint (CONSTRAINT-OF-TYPE-PARAMETER-CIRCULARITY)

`getConstraintOfTypeParameter` (checker.go:17059) answers nil when
`hasNonCircularBaseConstraint` fails (`<T extends T>`, `<U extends T, T
extends U>`): the type parameter is unconstrained for printing
(`typeParameterToDeclaration` writes no `extends`) and for relations. The
signature's `TypeParameter` carried the written constraint regardless, so
signatures printed `<T extends T>`. `type_parameter_of` now asks
`constraints.rs::constraint_of_type` (already `getConstraintOfTypeParameter`
with the circularity test) whenever a constraint is written and drops it on
`Nil`. TS2313 still reports once, from the same base-constraint resolution.
No cache beyond `base_constraint_cache`'s.

## 19. A JSDoc `@type` on `export default` types the default export (JS-DEFAULT-EXPORT-JSDOC-TYPE)

`reparseHosted` (parser/reparser.go:342) rewrites `/** @type {Foo} */
export default expr` into a cast of `expr` to `Foo`, so the `default`
symbol's type is the tag's. The port kept the gap here: an earlier draft
typed the tag and the importing file printed the local typedef name where
native writes `import("./a").Foo` (the §158 per-site naming wall). The
printer has since gained import-type spelling for most shapes, so the tag is
typed: `checkJsdocTypeTagOnExportAssignment2` and `exportDefaultWithJSDoc1`
convert; `checkJsdocTypeTagOnExportAssignment1/5/6` still print the local
`Foo` (they printed `any` before) and `exportDefaultWithJSDoc2`'s cast on the
expression itself still declines — both the printer's re-spelling, routed.

## 20. A written `any` constraint is `unknown` (TYPE-PARAMETER-CONSTRAINT-ANY-TO-UNKNOWN)

`getConstraintFromTypeParameter` (checker.go:17085) replaces a written
constraint that resolves to `any` (not the error type) with `unknownType`
(`stringNumberSymbolType` for a mapped type's key, which never reaches a
signature's type parameters). `type_parameter_of` now does the same for a
constraint that actually resolved — the unresolvable-constraint stand-in
`any` it already used is left alone, as native's `isErrorType` exclusion
leaves `errorType` — and drops the written `any` text, so `<T extends any>`
prints `<T extends unknown>`. Not ported here: the constraint's semantic
road (`constraints.rs::type_parameter_constraint`, member reads through
`T`'s apparent type in `typeParameterExplicitlyExtendsAny` 0:13/0:15), which
is that file's.

§18's circularity test is limited to a resolved constraint that is a type
parameter or an indexed access: native resolves constraints lazily, so a
constraint naming a value (`var j: <T extends typeof j>(x: T) => T`) never
forms a cycle while `j`'s annotation is built; asking about it eagerly
reported a TSR-only TS2313/TS2502 pair (`recursiveTypesWithTypeof`, refused
by the compare until the guard).

## 21. A missing global apparent type is `{}` (GLOBAL-TYPE-MISSING-LIB-FALLBACK)

`getGlobalType` (checker.go:1210) answers `emptyObjectType` for a missing
global, and `getApparentType` reads a primitive's members through it
(`globalBigIntType`, checker.go:21729), so a `bigint` without
`lib.es2020.bigint` has `{}`'s members — Object's, through
`getPropertyOfType`'s augment (`bigintVal.valueOf : () => Object`).
`members.rs::apparent_type` returned the primitive itself, which has no
members. It now returns the canonical empty object (`intrinsics.empty_object`,
the identity its `object`/non-strict `unknown` arm already uses) — when a
global `Object` exists: a checker bound with no lib at all (unit fixtures)
cannot relate the empty object to a `{}` literal (`relater.rs` answers
`Unknown`, routed), and the old primitive answer kept those fixtures
decidable (`mapped.rs`' `concrete_mapped_index_keys_…` test). Not here:
an array literal without a global `Array` (`createArrayType` over
`emptyGenericType`, `[1, 2, 3] : {}`; `noCrashOnNoLib`,
`arrayIterationLibES5TargetDifferent(nolib)`) is the array-literal road
(r7-contextual).

## 22. A static `prototype` property is the class's prototype (BIND-CLASS-PROTOTYPE-SYMBOL, partial)

`bindClassLikeDeclaration` (binder.go:962) gives every class a
`Property|Prototype` `prototype` export before its body binds. A written
`static prototype` *property* merges into it (property excludes are
empty), so `getTypeOfVariableOrParameterOrPropertyWorker`'s
`SymbolFlagsPrototype` arm (checker.go:16580) types the declaration
`getTypeOfPrototypeProperty` (the instance type, `any` per type parameter):
`static prototype;` is `C2`. A static method or accessor conflicts (TS2300)
and keeps its own symbol. This binder mints no prototype symbol; the
variable/property worker recognises the merge (a static `PropertyDeclaration`
named `prototype` whose parent node is a class) and answers
`members.rs::prototype_type_of_class`, the helper the static side's
`C.prototype` read (§117) now shares. Not ported: the binder-level symbol
itself, and computed names that late-bind to `prototype`
(`[FunctionPropertyNames.prototype]`, `staticPropertyNameConflicts` 65/195/330),
which `lateBindMember` resolves against the early prototype symbol.

## 23. Parameters and body variables are not visible from outside a function's body (RESOLVE-NAME-FUNCTION-LIKE-PARAMETER-SCOPE)

`resolveNameHelper`'s `useResult` block (binder/nameresolver.go:61-80) for a
function-like `location` reached from anything but its body: a type found in
its locals is visible only if it is a type parameter seen from the parameter
list or return type (§842, ported); a function-scoped *variable* (a
parameter or a `var` in the body) only from a parameter, or from the return
type when it is declared in a parameter. Only the type-parameter-list case of
the variable arm existed (§838), so `function bar(): typeof b { var b }`
resolved `b` (native TS2304) and a member's computed name `[foo<T>(a)]<T>(a:
T)` resolved `a` to the member's own parameter (native `any`, TS2552).
`function_scoped_variable_hidden_outside_body` is the arm; a JSDoc-kind
arrival is treated as native's synthesized reparsed node, which sees the
parameters. `useOuterVariableScopeInParameter` stays the caller's.
`functionVariableInReturnTypeAnnotation` converts (diagnostics);
`typeParametersAndParametersInComputedNames` now reads `a : any` and reports
TS2552, still missing TS2304 for the type argument `T`.

Perf: the arm runs on every successful local lookup at check time. As a plain
call it cost +0.28% Ir on domain-model; an always-inlined prelude (arrival
present and a function-like location) in front of an out-of-line remainder
that tests `Block`/`Parameter`/the JSDoc kind range before any table read
brings it to domain-model 1.0011, generic-imports 1.0000.
