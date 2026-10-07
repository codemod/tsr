# Alias-publication recovery (tsr-1yb.7.7.3)

Native pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Recovery base: `0e7824dd`.

## Accepted owned prerequisite

`binder.declareSymbolEx` (`internal/binder/binder.go`) creates the missing-name
symbol outside the scope table, then calls `addDeclarationToSymbol`. The Rust
missing-name branch had returned before publishing includes and the value
declaration. It now publishes both. This does not change name lookup or merge
identity; each missing declaration owns its own binder symbol.

`alias_publication_missing_name` constructs an absent namespace-import name
(not an empty parser identifier), proving ALIAS meaning remains available while
`__missing` is absent from locals. It fails on the recovery base and passes with
the fix. Direct native/TSR CLI diagnostics for a malformed namespace import
match; that parser control does not alone distinguish this binder boundary.

## Recovered callable/interface value boundary

After the parent published the recovery source, explicit fetch made
`origin/box/parity-symbols` and `97f99018` available. Recovered only its owned
`symbols.rs` hunk and dedicated semantic test, not its branch snapshot or
historical docs. Native `getTypeOfFuncClassEnumModuleWorker` and
`resolveAnonymousTypeMembers` use exports/signatures for the callable value;
merged interface instance members must not erase that value. The existing
non-interface JavaScript constructor/prototype unsupported boundary remains.
No cache, traversal or shared Checker field was added.

Current semantic regression fails before (prints error instead of
`() => number`) and passes after. Instance-only member is absent on the callable
value and present on the declared interface. Native direct control emits TS2322
and TS2339; the recovered port restores exact TS2322, but TS2339 remains a
separate diagnostic-owner prerequisite. This control is not fully passing.

Current full dumps: 19 gained RIGHT type rows, zero prior RIGHT losses and zero
vanished keys; 469,804/477,970 RIGHT, 983 GAP, 7,183 WRONG. Diagnostics unchanged.
Coverage: 8,052/9,538 type cases, 4,222/5,502 nonempty diagnostic cases.
`functionAndInterfaceWithSeparateErrors` gains its two emitted rows; contextual
inference 2/3/4 remain multi-root failures, not accepted case conversions.

Focused alias/cross-file/callable tests, checker all-target clippy and owned-file
format checks passed. Fresh-process new/binder-only-base median child CPU ratios
(21 samples) were 0.9833 domain-model and 0.9959 generic-imports; diagnostics,
scope and options matched. Observed wall ratios 1.0628/0.9962 are not verified
native complete-work ratios (those remain null). No release-target claim.

## Canonical alias cutover is not accepted

The saved branch and commit were initially absent; explicit fetch after parent
publication succeeded. The local Beads database does not contain
`tsr-1yb.7.7.3`. No replacement issue was created.

An experimental native resolveAlias/tryResolveAlias/resolveIndirectionAlias port
used validated `SymbolRef` keys and targets, with links owned by the existing
Checker resolution stack. Absent/unsupported, active stack work, completed
success and completed private unknown were distinct; no private raw index was
projected into binder IDs. Pure aliases recursively forced their target;
merged local meanings stopped indirection. Publication preceded failed-pop
TS2303 recovery. Type-only declaration identity propagated without overwriting
a nearer written marker. Completed targets reused the worker result rather
than repeating declaration/module resolution.

That cutover is **withheld**, not verified: it produced 17 previously RIGHT
diagnostic losses and 14 previously RIGHT type-line losses. Zero keys vanished.
The integration owner must first serialize these consumer prerequisites:

- `check.rs::type_only_alias_declaration`: replace its capped immediate-target
  walk/first-declaration read with canonical published declaration identity;
  preserve import/export policy from the marker's syntax kind.
- Alias circularity consumers: preserve diagnostic publication for all
  participants, including already-resolved cross-file cycles. Eight losses were
  `recursiveExportAssignmentAndFindAliasedType1..6` and `circular1/3`.
- Module/default alias consumers and printing: retain written alias/receiver
  context independently of canonical terminal targets. Lost controls included
  `moduleAugmentationDuringSyntheticDefaultCheck`,
  `nodeNextCjsNamespaceImportDefault1/2`, `chained2`, `exportDefault`, `generic`
  and `implementsClause`.

No heuristic, suppression, compatibility shim or rejected semantic patch is
included in the accepted commit. Callable merged-interface port
`97f99018` has now been recovered and remeasured independently above.

## Resolutions-owned canonical implementation experiment

The integration owner subsequently accepted one alias map attached to existing
Resolutions, with the same private Checker lifetime; no Checker field is needed
for that placement and no second raw-ID cache is permitted. The owned experiment
is saved as `canonical-alias-consumer-cutover.patch` in ignored receipts, not
committed as a production cutover.

Actual consumer controls passed for failed-pop unknown publication of every
cycle participant, completed-unknown reuse without duplicate diagnostics,
tryResolveAlias active absence without poisoning frames, and type-only-origin
propagation distinct from immediate written target. Source-file diagnostic
ownership was corrected through Checker::report; this fixed the previous eight
circularity RIGHT losses.

The full experiment still lost 13 previously RIGHT diagnostic cases and 14
previously RIGHT type rows; no keys vanished. It cannot integrate until the
parent serializes check.rs type-only/hidden-module consumers and printing/default
alias context. Required type-only consumer replacement: read the published
origin, classify ExportSpecifier/ExportDeclaration/NamespaceExport as export,
and remove the capped first-declaration walk. Hidden-module consumers must use
written immediate target rather than canonical terminal presentation. Native
complete-work/performance acceptance was not measured for this rejected cutover.
Strict clippy on the experiment also reported four style failures; no clean
canonical quality-gate claim. Applied experimental code was removed from the
production branch, preserving its verified callable/interface fix.

## Exact serialized alias-state interface

Integration owner alone applies these two Checker additions with the canonical
consumer cutover:

```rust
pub(crate) alias_symbol_links:
    FxHashMap<crate::symbol_access::SymbolRef, crate::resolution::AliasSymbolLinks>,
// with_module_host constructor:
alias_symbol_links: FxHashMap::default(),
```

Owned resolution state uses existing `SymbolRef` (no new identity space):

```rust
#[derive(Debug, Default)]
pub(crate) struct AliasSymbolLinks {
    pub(crate) alias_target: Option<SymbolRef>,
    pub(crate) immediate_target: Option<SymbolRef>,
    pub(crate) type_only_origin: Option<NodeId>,
}
```

Native writers and reentry, audited at the pin:

- `resolveAlias`: nil target enters shared AliasTarget resolution; push failure
  returns private unknown without publishing completion. Pure nonlocal aliases
  recurse. Target publishes before pop; failed pop reports TS2303 then overwrites
  with unknown. Completed unknown remains distinct from absence.
- `tryResolveAlias`: completed target forces ordinary resolver; unpublished
  target with no cycle-start frame does likewise. Active cycle-start returns
  absence without failing any frame. Search checks published properties before
  matching the requested frame and respects resolutionStart.
- `resolveIndirectionAlias`: merged canonical target plus first-writer-wins
  type-only-origin propagation. Receiver/written origin is not this target.
- `markSymbolOfAliasDeclarationIfTypeOnly`: syntactic origin first, otherwise
  type-only export-star origin; neither overwrites an existing marker.
- `getImmediateAliasedSymbol`: independently caches getTargetOfAliasDeclaration.
  Its written edge must not be replaced by the canonical terminal target.
- `createDefaultPropertyWrapperForModule`: seeds a private synthetic alias's
  completed canonical target directly. This writer requires SymbolRef end to end.

Resolved/Unsupported result variants keep unsupported port work separate from
native unknown; try additionally returns Active. Unsupported never writes
alias_target. No raw-private-index conversion or bound compatibility getter.
Recover-alias declared consumers need this real distinction, not another
speculative F6P0 field.

The minimal patch in ignored recovery receipts passes `git apply --check`.
Definitions are delivered beside it, not installed as dead compiled state.
Installation requires the integration owner's shared field and atomically
migrated consumers: this worker cannot apply checker.rs, check.rs, declared.rs
or printing.rs. The previously measured RIGHT losses prohibit an isolated
replacement of the existing raw-ID resolver.

## F6 signature-container admission

The declared-alias owner identified the binding-parent prerequisite. The parent
implemented direct binding_pattern_implied_type demand for unannotated,
initializer-free type-only signature parents in destructure.rs, matching native
getTypeForVariableLikeDeclaration. This bypasses the symbols admission gate;
the duplicate symbols gate change in 4f011bae was removed. No independent
symbols consumer needing that broadening has been established. Contextual
expression admission remains unchanged; no new cache or any/name fallback.

The parent exclusively owns destructure.rs binding-parent repair. This admission
hunk alone does not claim to fix F6 typeof-renamed type production. Alias and
cross-file release tests passed; the actual rebuilt CLI's diagnostics matched
native on function/constructor type patterns with typeof-renamed returns. That
no-diagnostic control does not prove returned type parity. Parent must exercise
the F6 type control after its binding-parent consumer edit.

## Attributed default-import usage-mode targets (tsr-2zk.16.161)

Claimed the existing Beads root; no duplicate task. Recovered only the coherent
owned symbols hunk and semantic mode-target test from f0ca4bbf after native
review. getTargetOfImportClause resolves its actual parent module specifier even
with attributes; getTargetOfModuleDefault then selects the target. Removed the
attribute-presence decline, not attribute diagnostics or validation.

Identity remains the existing Program importing SourceFile/specifier/usage-mode
resolution. Explicit import versus require mode selects different source files
for the same package string; source/default target and written importing alias
remain distinct. The ModuleHost already supplies actual usage mode. No new
cache, mapper, identity space or eager traversal. Expensive module resolution
remains the Program's existing resolved-module lookup; this checker change is
one admission branch, not a second name-keyed cache.

Native direct fixture uses package conditional types exports: ESM default is
`'esm'`, CJS export= is `'cjs'`. Both wrong-mode assignments
emit exact TS2322 messages/spans/order in native. The frozen pre-fix TSR emitted
none; rebuilt TSR output is byte-identical to native. The focused canonical
Program-identity/mode test fails before and passes after.

Current target emitted rows before -> after RIGHT: importAttributes11 3/4 ->
4/4; importAttributes7 20/21 -> 21/21; importAttributes8 6/7 -> 7/7;
resolutionModeCache 6/8 -> 8/8. Coverage gains three whole type cases, not four:
8,055/9,538; diagnostics unchanged 4,222/5,502. Full verdicts have zero previously
RIGHT losses and zero vanished keys; five added RIGHT rows relative to callable
recovery (469,809/477,970 RIGHT, 983 GAP, 7,178 WRONG).

Focused alias/cross-file/callable/mode tests, all-target checker clippy and owned
format checks passed. Fresh-process median child CPU new/base ratios over 21
samples: domain-model 1.0173, generic-imports 0.9867. Diagnostics/scope/options
matched; observed wall 1.0068/0.9820, verified native complete-work wall null.
Issue remains open for parent integrated full-configuration acceptance; neither
99.9% exact parity nor native equivalent complete-work wall <=0.50 is claimed.

## String-literal external module members (tsr-2zk.16.234)

Claimed existing root. Native getExternalModuleMember accepts identifiers and
string literals and uses decoded Text; empty string is explicitly valid. The
owned resolver now passes either spelling to its existing module export/member
lookup. No new cache/traversal/identity or name heuristic; written alias and
export receiver remain unchanged. Empty and dashed keys stay distinct.

Direct native control imports empty/dashed exports, then assigns both numbers
to strings: pre-fix TSR emitted nothing; rebuilt output exactly matches native's
two TS2322 messages/spans/order. Semantic test fails before (error instead of 1)
and passes after. Full verdicts lose zero prior RIGHT keys and vanish zero keys.
Relative to attributed-import recovery: +5 RIGHT type rows, +2 whole type cases,
+1 whole diagnostic case. Targets bigintArbirtraryIdentifier 17/20 -> 20/20 RIGHT;
arbitraryModuleNamespaceIdentifiers_exportEmpty 5/7 -> 7/7 RIGHT. Current
coverage types 8,057/9,538, diagnostics 4,223/5,502; total type RIGHT
469,814/477,970 (983 GAP, 7,173 WRONG).

Focused string-name, cross-file, callable and attributed-mode release tests,
checker all-target clippy and owned format checks passed. Linux fresh-process
median child CPU new/base over 21 samples: domain-model 1.0080, generic-imports
0.9863; diagnostics/scope/options matched. Native complete-work target remains
unverified; this bounded port is not a campaign parity/performance certification.

## Receipts

Box receipts live under ignored `target/recovery/recover-symbols/`, including
base/current full verdict dumps, native CLI controls, failing-base/passing-fix
binder regression logs, and `canonical-alias-rejected.patch`. The parent must
copy this directory before machine destruction; ignored files are not carried
by the commit.

For the accepted binder-only change, both full dumps preserve every previously
RIGHT key, including missing-key comparison. Diagnostic results unchanged:
9,190/10,570 RIGHT or EMPTY_RIGHT. Type-line results unchanged:
469,785/477,970 RIGHT; 995 GAP; 7,190 WRONG. These historical baseline-suite
verdicts are not a claim of exact native parity across every configuration.
The coverage executable was rebuilt from the accepted binder-only source in
an isolated worktree, so snapshot writes did not touch protected source-tree
snapshots: checker_types 8,051/9,538 cases (84.41%, 98.11% lines), diagnostics
4,222/5,502 nonempty cases (76.74%). This is not a full-configuration oracle.

Binder/checker release tests passed; binder all-target clippy and owned-file
rustfmt checks passed. Workspace release testing failed four
`module_default_file_owner` tests (`chains_past_the_naming_bound_still_resolve`,
`defaults_share_the_original_module_not_its_default_property`,
`direct_real_defaults_still_name_the_callable`,
`every_alias_shape_resolves_to_the_immediate_export_equals`). Their baseline
status was not rechecked; no workspace pass is claimed.

Fresh-process self-comparison against the frozen recovery-base CLI had matching
diagnostics/scope/options. Median child CPU new/base ratios: domain-model
1.0129 (41 samples; 21-sample result exceeded 1.03), generic-imports 0.9796
(21 samples). Observed wall ratios were 1.0689 and 1.0180; verified equivalent
native-work wall ratios are null. The public projects exited with one diagnostic
and are not evidence of the campaign's complete-work target.

The >=99.9% campaign target and equivalent complete-work median wall <=0.50
remain unverified.
