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
