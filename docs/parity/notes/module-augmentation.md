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

## Augmented-symbol import-type root: integration contract

Pinned native `internal/checker/symbolaccessibility.go` supplies the semantic
container algorithm, not the node builder's rendered name:
`getContainersOfSymbol` → `getWithAlternativeContainers` →
`getFileSymbolIfFileSymbolExportEqualsContainer` / `getExternalModuleContainer`,
with `getSymbolIfSameReference` in `internal/checker/checker.go`.

Existing producer metadata is sufficient for the immediate root identity:
Symbol.parent, Symbol.declarations and Symbol.export_symbol are Program-store
SymbolIds/NodeIds; BindResult.merged_symbol supplies completed merge redirects.
Do not introduce a second parent/root cache or store a printed import specifier
as identity. Native getParentOfSymbol canonicalizes the parent (and forces its
late-bound symbol); native same-reference comparison canonicalizes both sides,
resolves aliases, then canonicalizes the resolved targets. A comparison of raw
parents or equal names is not that algorithm.

Exact cross-owner serialization request to integrator/parity-printing:

1. Agree the semantic producer API before wiring the printer. The native-shaped
   candidate is `get_containers_of_symbol(symbol: SymbolId,
   enclosing: Option<NodeId>, meaning: SymbolFlags) -> Vec<SymbolId>` in owned
   symbols.rs. Result order is significant; this is *not* an API for one guessed
   import-type root. The enclosing declaration and meaning must remain inputs.
   Implement the whole reachable native contract, not a printer-specific shim.
2. `getWithAlternativeContainers` preserves the real namespace first when an
   accessible namespace chain exists, otherwise prefers export-equals file
   containers before the real parent. It appends reexport and object-literal
   alternatives, and considers instance-side variables only for native VALUE
   meaning. `getAlternativeContainingModules` uses imports in the enclosing
   file and getAliasForSymbolInContainer; this requires the accessible-symbol
   chain/alias producer to be audited before a faithful full API is published.
3. For an export-equals namespace member, derive the alternative module from
   the parent's actual declarations and ancestor module node, then compare its
   export-equals target by native same-reference semantics. This returns the
   ambient or SourceFile *module SymbolId*, preserving the merged function /
   namespace's own identity. Neither the augmentation's string-literal name nor
   the target namespace's text independently certifies that container.
4. Printer owner consumes ordered semantic container candidates and retains the
   written alias/origin/enclosing declaration for import-type rendering. The
   existing `printing.rs::export_equals_class_text_at` is class-only and derives
   a SourceFile container; it cannot establish the augmented ambient namespace
   member contract for augmentExportEquals5 or angular. Do not broaden it using
   its rooted single-directory string rendering as semantic evidence.
5. If memoization is required, native symbolContainerLinks.extendedContainersByFile
   is privateChecker-owned, keyed by canonical SymbolId then enclosing SourceFile
   NodeId; options and bound Program are fixed for that checker. Absent versus
   completed empty must be distinguishable. Do not publish an active recursive
   assumption as completed. Actual alias/container/accessible-chain worker counts
   are unmeasured here: request a bounded Beads follow-up before extending reuse.

No unused candidate API or checker state field is added in this lane commit.
The integration owner must serialize symbols producer, checker state if needed,
accessible-chain dependencies, and printing consumer together. This keeps a
partial implementation from claiming to have native container ordering.

Integration update reports current main coverage 8046/9538 types over 12444
source cases. That is not this worker's frozen/verified 8042/9538 snapshot; the
counts below remain attributed to the measured parent. Expanded/native-only
configuration coverage belongs exclusively to parity-full-corpus.

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

### Final publication and missing-key evidence

The completed root implementation is commit `ebc34de0`; integration contract
handoff is `bfe0e755`. The frozen release TSR SHA256 is
`866e1c6ebe89bf98af5cd20e04fb44b93bb4f73a4cd3d84566c6021c0893211b`;
the locally built pinned native binary SHA256 is
`7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`.

Native completion for this root: resolveExternalModuleSymbol(false) obtains
the terminal alias target, getMergedSymbol selects its canonical symbol,
mergeModuleAugmentation checks Namespace meaning, and mergeSymbol publishes the
successful source redirect using recordMergedSymbol. TSR resolves its supported
external import-equals target chain before the same namespace check and calls
Binder::merge_pairs only for accepted targets. The regression queries Item
through bound.merged_symbol after merge_module_augmentations returns and observes
both original and added members, while the unrelated Item stays distinct. The
circular chain returns no target and no member publication. This proves the
supported completed binder member image, not privateChecker alias-link
publication or all native merge/diagnostic behavior. Existing Binder::merge_symbol
publishes its redirect before recursive member merging rather than after it;
binding is exclusive and consumers run after the returned BindResult. That
existing ordering is not claimed equivalent for checker re-entry.

Final explicit key-set comparison, treating actual verdict rows as keys: types
477970 baseline/candidate keys, diagnostics 10570 baseline/candidate keys;
missing=0 and extra=0 for both. The type dump contains multiline payloads, so
physical line count is not the key denominator. Both complete byte streams are
identical, stronger than the RIGHT-only loss check for this existing oracle.
Coverage reports all 12444 discovered cases and writes its ordinary snapshots;
no snapshot changes are committed. No strict expanded/variant native oracle
completion is inferred from these normalized runs.

### Ambient export-default local name follow-up to closed .16.24

Assigned bounded follow-up: tsr-2zk.6.17 (created/claimed by integrator).
Default-local lookup drops ambient module context because this parser does not
publish native NodeFlagsAmbient. Completed tsr-2zk.16.24 remains closed. The existing arm is already implemented;
this follow-up restores its exact context prerequisite, not its historical
missing algorithm.

Pinned native binder NameResolver.Resolve, GetLocalSymbolForExportDefault:
locals/shadowing are consulted first; SourceFile/ambient nonglobal module then
looks up default, requires syntactic default on the first declaration, actual
local-symbol name equality and result flags intersecting requested meaning.
The binder already computes ambient module context from enclosing ambient,
declaration file and declare modifier. It now publishes that existing fact as
NodeFacts::AMBIENT_MODULE_CONTEXT and the default-local arm reads it alongside
native parser AMBIENT. No rendered-name identity, alias fallback or new semantic
cache is introduced. Existing marker.export_symbol and canonical merged default
SymbolId are retained. Namespace meaning must still reject a plain class; local
parameters still shadow the default-export name.

Owner/lifetime: one Program BindResult NodeFacts table keyed by module NodeId.
Absent means no binder-proven ambient context; binding publishes the fact before
children, consumers run after binding completion. There is no active semantic
answer/failure cache. The context is the same ambient bit already used by member
binding, including declaration-file state. Expensive work remains the existing
scope walk/default lookup, with one additional bit lookup for module queries.
Actual worker/query attribution remains unmeasured; integrator follow-up request
before widening reuse. No optimization claim.

Native controls: pkg.d.ts declares module pkg, export default class Foo with
value:number, and export {Foo as Named}; main imports Named and assigns its
instance value to string. Native and candidate emit sole TS2322 main.ts(4,7),
complete message `Type 'number' is not assignable to type 'string'.` Frozen TSR
emits nothing. Regression asserts VALUE/TYPE lookup reaches actual default
SymbolId, NAMESPACE rejects it, a parameter shadows it, and missing name misses.
The corpus-derived ambient before/after C reference control is native clean
under ES2015; frozen TSR emits three TS2304 errors, candidate is clean. Native
ES5 invocation rejects target with TS5108; no ES5 native parity claim.

Verification: full unfiltered keys types 477970/477970, diagnostics 10570/10570;
zero missing keys and zero formerly RIGHT/EMPTY_RIGHT losses. Type counts remain
469765 RIGHT, 7212 WRONG, 993 GAP. Diagnostic EMPTY_RIGHT grows 4968 to 4969,
EMPTY_WRONG drops 100 to 99: compiler/es5ExportDefaultClassDeclaration4 converts
on the existing normalized oracle. Full 12444-source coverage completes,
checker_types 8042/9538 and diagnostics 4221/5502 unchanged; generated snapshots
are not committed. Workspace release tests pass; strict clippy and format pass
after removing one unused test import; native anchor gate passes. No other
historically named target is claimed converted.

Fresh-process interleaved 21-pair candidate/frozen baseline observed wall ratios
domain-model 0.991740, generic-imports 0.998594; median child CPU ratios 1.010905
and 0.991988. Against pinned native 21-pair observed wall ratios 1.019439 and
0.921646. Harness work_comparable=false: complete query-input coverage and
performed checker budgets remain unverified; no <=0.50 verified release ratio.

### Alias-indirection next-root prerequisite (.16.68)

Current issue tsr-2zk.16.68 is absent from the Box DB (`bd show` reports not
found), so its 16 current named cases cannot be recovered locally. Native ad
hoc reproduction is direct and does not depend on historical triage: namespace
Root exports value:number; import a0=Root; eleven aliases through a10; then
const good:number=a10.value and const wrong:string=a10.value. Pinned tsgo
reports sole TS2322 at main.ts(14,7), complete message `Type 'number' is not
assignable to type 'string'.` Current rebuilt TSR reports no diagnostics.

Pinned Checker.resolveAlias uses aliasSymbolLinks[symbol].aliasTarget as
completed publication. An absent target pushes TypeSystemPropertyNameAliasTarget,
gets the alias declaration target, transitively resolves a non-local pure alias
through resolveIndirectionAlias, and stores target or unknownSymbol before pop.
Failed pop emits TS2303 at that alias declaration and replaces the published
target with unknownSymbol. A re-entered active resolution returns unknownSymbol;
it is not a completed successful cache hit. resolveIndirectionAlias canonicalizes
the resolved target using getMergedSymbol and back-propagates the target's
non-null typeOnlyDeclaration only when the source origin is still absent.

Exact integration prerequisites (outside this lane's owned files):

- checker.rs::Checker fields/constructor: privateChecker-owned alias link store,
  keyed by SymbolId in its fixed Program SymbolStore, holding completed target
  or unknown and propagated type-only origin NodeId. Active ownership must use
  the existing native resolution-stack semantics, not a thread-local/static
  cache or type cache. Unsupported implementation work must remain distinguishable
  from a completed native unknown; current resolve_alias Option collapses those.
  No cache lifetime may outlive the Checker/Program/options context.
- circular_alias.rs::check_circular_import_alias, alias_chain_returns_to and
  alias_recursion_target: migrate the independent immediate-target diagnostic
  traversal to native resolveAlias's failed-pop publication. The current helper
  has its own 64-hop cap; retaining it after native recursion would duplicate
  or relocate TS2303 and lose aliases leading into versus participating in a cycle.
- check.rs::type_only_alias_declaration and its consumers: consume propagated
  native type-only origin rather than re-walking immediate targets with a 16-hop
  cap. check.rs::alias_chain_carries and other capped target consumers also need
  coordinated migration when resolve_alias changes from immediate to terminal.
- symbols.rs (owned): split immediate getTargetOfAliasDeclaration dispatch from
  resolveAlias publication, add resolveIndirectionAlias, remove the artificial
  resolve_alias_fully 8-hop cap, and apply merged canonicalization after target
  completion. Bare internal import-equals must resolve alias Namespace meaning
  through the native entity-name operation instead of requiring a module type
  clone as current code does. That implementation waits on the link-state and
  diagnostic/type-only consumer cutover above; no parallel alternate resolver.
- resolution.rs (owned): add native AliasTarget property only with its real
  push/pop consumer. Every exhaustive property consumer outside ownership must
  migrate in the same serialized cutover; adding a dead variant is not a port.

Required API decision: whether resolve_alias remains Option<SymbolId> externally
with an internal explicit result state, or cuts over to an explicit
AliasResolution result. In either design callers must distinguish completed
unknown from unsupported, preserve written alias/origin for presentation, and
never use printed names as keys. The integrator must serialize all affected
callers; no compatibility shim is proposed. Actual worker/count boundaries for
alias queries, completed hits, active repeats, target executions and type-only
propagation are unmeasured: request a bounded Beads follow-up before reuse grows.

No implementation changes or permanent tests are committed for this blocked
root. The ad hoc reproduction is scratch-only. The previously verified binder
root's full unfiltered/loss/performance evidence does not certify this unported
alias algorithm or convert any of the 16 unavailable target cases.

### Declaration-name/module-name next-root prerequisite (.16.117)

The Box issue DB returns not found for tsr-2zk.16.117, so its current case list
cannot be attributed or validated from that database. Existing six target
names and measured verdicts remain recorded above; no new historical-case gain
is assumed.

Read-back evidence: types_producer.rs::type_at_location's declaration-name
branch already queries get_type_of_symbol(binder.merged_symbol(symbol_of(parent))).
The binder symbol_of API deliberately returns the raw node binding. Native
getSymbolAtLocation's IsDeclarationNameOrImportPropertyName branch calls
getSymbolOfDeclaration(parent), whose exact sequence is node.Symbol(),
getLateBoundSymbol, then getMergedSymbol. Thus replacing the raw binding getter
or adding another resolver workaround is not a faithful fix.

Exact exported contract request: integrator serializes a checker-level
`get_symbol_of_declaration(node: NodeId) -> Option<SymbolId>` returning the
native completed late-bound/merged declaration symbol. Input is a Program NodeId;
output is a SymbolId in the same Program store. Non-computed declarations use
the existing bound symbol and completed merge redirects. Computed class-member
names must force the native late-bound members/exports worker before claiming a
completed answer. Consumers requiring raw binding keep BindResult::symbol_of.
The declaration-name/module-name type consumer in conformance types_producer.rs,
checker semantic location queries, and printer owner must migrate together;
this owner cannot edit those files. Checker state/completion dependencies must
be supplied by their owner; do not introduce an unused API or guessed result.

For augmented symbol import-type roots the separate getContainersOfSymbol
contract above remains required. Neither the declaration-symbol query nor a
module resolved-export table substitutes for accessibility/container ordering.
The derived/resolved exports host contract tsr-2zk.16.59.1 is pending its single
owner after current file release; no implementation or new host contract for it
is added here. Current supported binder import-equals root remains the only
implemented/verified root in this lane.

Next owned root is export-star resolved-export identity/publication. Current
export_star_member is a name-at-a-time traversal which drops collisions;
native getExportsOfModuleWorker constructs the whole export table, keeps the
first colliding target, records TS2308 collisions, and preserves type-only
star metadata. A faithful next port needs integrator agreement on one exported
semantic table result (ordered diagnostic events and type-only origins alongside
SymbolIds), plus the checker state/diagnostic consumers outside ownership. It
must not publish module-only results as printer accessibility results. No partial
replacement or duplicate speculative cache is committed while that contract is
unresolved.

The external import-equals target boundary is implemented and exercised. The
full assigned cluster and release goal remain blocked by the cross-owner
prerequisites above; integration must not mark tsr-2zk.38 closed on this commit.
