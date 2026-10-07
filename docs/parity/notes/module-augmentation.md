# Module augmentation: tsr-2zk.38

## Pinned operation and boundary

Native: typescript-go `5b1047d10d32e7d5b446be4de56b126ff42f82bb`,
`mergeModuleAugmentation` → `resolveExternalModuleSymbol(false)` →
`resolveAlias` / `resolveIndirectionAlias` → `getTargetOfImportEqualsDeclaration`.
The consumer is the non-global augmentation target, before its namespace-meaning
check and before export-star/member merging.

The binder-owned port follows identifier export assignments and external
import-equals chains. Every module query uses the existing Program callback,
with the importing SourceFile NodeId and actual string-literal usage NodeId;
Program chooses the usage resolution mode. It does not match unit names or
printed symbol names. Symbols and merge redirects belong to one Program
BindResult/store, after all files bind, and live for that Program. Options stay
with Program; this operation introduces no option-independent semantic cache.

The active vector contains store-local SymbolIds for this query only. Absence
means unvisited; a repeated active alias is resolution failure, not provisional
success. A terminal symbol is success; unresolved modules and unsupported alias
syntax return no target. Only successful namespace targets enter the existing
merge worker. Nothing publishes checker alias links, type-only markers or
completed failure entries. No receiver/type image is manufactured. Namespace
imports remain unsupported here because native resolveESModuleSymbol can build
an interop-sensitive module image; substituting its underlying namespace is not
faithful.

Expensive work is the alias declaration walk and the existing Program module
lookup, followed by Binder::merge_symbol. The walk allocates its active path
only for modules containing export-equals; no new memoization is added. Actual
worker/count attribution is unmeasured. Integrator Beads follow-up request:
record augmentation target queries, alias hops, active repeats and merge worker
executions under tsr-2zk.38 before widening reuse. This correctness port makes no
speed claim.

## Reproduction

Frozen source parent: `5dd3bad84d12991e1ba169d2d5687321e1989740`.
Unfiltered frozen dumps: 10,570 diagnostic rows and 477,976 aligned type rows.
The release binary and dumps are `/tmp/box/base` on the disposable worker.

Native-supported control: core exports a function/namespace with Item.original;
bridge imports core using import-equals and exports that alias; main augments
bridge with Item.added:string, consumes both properties and assigns added to
number. Pinned tsgo reports only TS2322 at main.ts(6,7), complete message
`Type 'string' is not assignable to type 'number'.` Frozen TSR instead reports
TS2339 at (5,28) and (6,28), missing the augmentation. Permanent binder regression
asserts the merged actual namespace member table, an unrelated Item remaining
distinct, and a circular bridge failing without publishing a target.

## Cross-owner prerequisites

- `crates/tsr-compiler/src/lib.rs`, Program::merge_module_augmentations: target
  callback is already available; no signature change required for this slice.
- Namespace-import augmentation targets need native resolveESModuleSymbol and
  its originating-import/module-image context, not a binder symbol-only
  substitution. `moduleAugmentationDuringSyntheticDefaultCheck` uses that form.
  Its duplicated strftime signatures require both augmentations to reach the
  native merged namespace. Synthetic default naming/printing also needs the
  printer owner (`printing.rs`/node-builder consumers).
- `crates/tsr-checker/src/declared.rs`, get_declared_type_of_symbol: audit incoming
  symbol canonicalization before cache lookup/worker dispatch. The function
  itself accepts raw symbols; merge redirects alone do not certify every
  consumer. This is an unverified boundary requiring a serial owner audit,
  not a speculative edit.
- `crates/tsr-checker/src/check.rs`, check_module_augmentation_name: currently
  validates existence only, not the full native augmentation target/diagnostic
  algorithm. Import declaration/local-name conflict checking must reproduce
  native checkImportBinding (TS2440) for
  conflictingDeclarationsImportFromNamespace1/2; return inference/checking must
  supply TS7023. augmentExportEquals5 additionally lacks native TS2454 definite
  assignment and import("express") type printing. These are not target merge
  diagnostics and are outside the owned lane. A pinned circular import-equals
  control reports TS2671 at the augmentation name plus TS2303 on the import and
  export; rebuilt TSR instead reports TS2708 at the value use plus the same two
  TS2303 diagnostics. Thus target failure does not yet have native diagnostic
  context in the checker; failing the binder target is not full cycle parity.
- Native getExportsOfModuleWorker/extendExportSymbols keeps the first colliding
  export and diagnoses TS2308; current binder export_star_member drops ambiguous
  names instead. Faithful replacement requires diagnostic consumers/order and
  type-only export-star context coordinated with checker check.rs/state owner.
  Do not describe this historical helper as the native algorithm.

No target case is claimed converted merely because it already has EMPTY_RIGHT
diagnostics. Full-corpus dumps compare only the existing normalized oracle;
complete message/chains/length/order and full variant coverage need the exclusive
conformance-owner strict oracle.

## Verification after the external import-equals port

- Actual rebuilt TSR CLI matches the pinned native control's sole diagnostic:
  TS2322 at main.ts(6,7), complete string-to-number message. Both binder tests
  pass; workspace release tests pass, clippy workspace/all-targets with
  `-D warnings` passes, and workspace format check passes after formatting only
  the two owned Rust files. Toolchain: installed Rust 1.96.0, Go 1.26.8; no
  offline bootstrap or tracked setup/lockfile edits were needed. Native anchor
  validation (`cargo run -p xtask -- anchors`) checks 4496 references with zero
  unresolved. `cargo xtask` itself is unavailable because this Box has no alias;
  the explicit package invocation succeeds.
- Full unfiltered diagnostic and type dumps complete and are byte-identical to
  frozen dumps (`cmp`), including multiline output. Thus zero formerly RIGHT
  line/case or EMPTY_RIGHT diagnostic losses, and zero measured corpus gains.
  Coverage completes: checker_types 8042/9538, matched lines 469765/478855;
  diagnostics 4221/5502; clean EMPTY_RIGHT 4968/5068. These are the normalized
  oracle's populations, not strict full variant/message/order acceptance.
- All six assigned names exist as compiler cases. Their combined type verdict
  remains RIGHT 82, WRONG 32, GAP 1 (115 lines). Diagnostics remain three WRONG
  and three EMPTY_RIGHT. No assigned case is claimed converted.
- Fresh-process interleaved candidate/frozen baseline: 21 pairs observed wall
  ratios domain-model 1.007925, generic-imports 1.009439. Domain-model child CPU
  noise exceeded 1.03, so both were repeated with 41 pairs: observed wall ratios
  1.004005 and 0.997480; median child CPU ratios 0.994844 and 1.004029. Both
  comparisons have equal listed scope/options and matching stable diagnostics.
  This does not prove the strict zero wall slowdown criterion for domain-model.
- Against pinned native, 21 pairs observed wall ratios are 0.994322 and
  0.927620 (domain-model / generic-imports). Equal listed scope/options,
  diagnostics stable/matching, inputs unchanged. The harness reports
  work_comparable=false and verified_wall_ratio=null: complete query-input
  coverage and actual performed checker-worker budgets are unverified. These
  measurements do not meet or certify the <=0.50 release target.

The external import-equals target boundary is implemented and exercised. The
full assigned cluster and release goal remain blocked by the cross-owner
prerequisites above; integration must not mark tsr-2zk.38 closed on this commit.
