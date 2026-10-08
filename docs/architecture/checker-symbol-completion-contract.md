# Node-symbol completion contract

Characterized for `tsr-1yb.4.1.6` at TSR `4cc9fcc5`, against exact native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. This is a representation blocker
and implementation handoff, not a shipped cache or optimization gain.
The measured opportunity remains in
[symbol-resolution performance](checker-symbol-resolution-performance.md).

## Completion state and publication

The native anchors below are functions in `internal/checker/checker.go` at
the pinned source. `symbolNodeLinks.resolvedSymbol` belongs to one Checker.
Absence, a completed unknown and a completed unresolved type are different
states. They cannot all be represented by a failed optional binder lookup.

| Getter and entry state | Resolution and publication | Repeated access |
|---|---|---|
| `getResolvedSymbol`, nil | Resolve a nonmissing identifier with Value/ExportValue meaning, missing-name diagnostics and use/write context. Publish the symbol or private `unknownSymbol`. | Return the completed entry, including unknown; do not repeat resolution or its diagnostic. |
| `getResolvedSymbol`, missing syntax | Skip name resolution; publish unknown. | No new missing-name diagnostic. |
| `getReferencedValueOrAliasSymbol`, nonunknown completed | Return existing completion. | Preserve written alias identity when that is the completed symbol. |
| Alias getter, nil or unknown | Resolve Value/ExportValue/Alias without missing-name diagnostics. **Do not publish** the fallback. | Run the fallback again; an unknown entry remains unknown. |
| `getSymbolFromTypeReference`, nil | Call `resolveTypeReferenceName` with Type meaning and errors enabled. Publish the fully resolved target, unknown, or synthetic unresolved symbol. | Return that completion. |
| Missing type name, errors enabled | `getUnresolvedSymbolForEntityName` interns a private unresolved TypeAlias symbol by its entire parent-qualified path. Its declared type is unresolvedType. | Two nodes can share the synthetic symbol while each node still performs its first diagnostic-producing lookup. |
| Missing type name, errors ignored | `resolveTypeReferenceName` can return unknown instead. | This is a different query contract; do not reuse an errors-enabled fallback indiscriminately. |

The nil state is uncomputed; it is not a negative result. Completion is tied to
the AST node and its supported getter context, not printed spelling. Distinct
`Missing.Child` nodes share a checker-local synthetic symbol after their own
lookups. `Other.Child` has a different child and parent identity. Type arguments
belong to the instantiated type, outside that static parent-path symbol key.

`getTypeFromTypeReference` has a separate type-node completion. A const type
reference in an assertion calls `checkExpressionCached` before ordinary symbol
lookup. Directly calling the symbol getter for `as const` in the probe returns
an unresolved symbol without a missing-name error; that is **not** the normal
const-assertion type-query path. Preserve the bypass.

## Ownership and current Rust representation

| Identity | Native owner | Current TSR boundary |
|---|---|---|
| Ordinary bound declaration | Program binding; the tested declaration is shared between two Checkers | `tsr_binder::SymbolId` indexes the immutable bound SymbolStore. |
| Unknown sentinel | Private Checker | Production CheckerSymbols owns a private Property/Transient unknown handle, distinct from node uncomputed state. |
| Unresolved path and its parents | Private Checker synthetic symbols | Production CheckerSymbols interns full-path private TypeAlias/Transient records with Unresolved CheckFlags, parents and distinct unresolvedType linkage. The reference consumer uses the symbol path, then creates the existing error-like presentation TypeId; error-type alias-owner migration remains .7.7.2. |
| Merged/cloned transient symbols | Checker preparation/resolution as required by native | Production clone/redirect accessors preserve private ownership and shallow symbol edges; table-presence fidelity is .7.7.1.1 and full augmentation preparation remains `tsr-6.49` and `tsr-1yb.3.2`. |
| Expression/type completion | Private Checker | `node_types`, `symbol_types` and instantiation tables store TypeIds. They do not substitute for static symbol completion. |

Rust's opaque SymbolId contains a store index without a domain tag. SymbolStore
holds borrowed names, declaration ids, members, exports and parent SymbolIds;
its symbols lack native transient CheckFlags. Checker borrows Binder
immutably. Appending synthetic ids to Program storage or passing private ids to
`binder.symbols().get` would violate these boundaries.

The design for **`tsr-1yb.4.1.6.1`** follows. It is a checked ownership layout
and consumer migration contract. The production storage foundation now implements
its domain validation and unresolved-reference path; storage task .7.7.1 stays
open for native absent/empty bound-table fidelity and further cost attribution. Consumer
migration, alias completion and retained node reuse are .7.7.2/.7.7.3/.7.7.4.

## Handle and storage layout

Use one opaque, nongeneric `SymbolRef` with two variants: a bound Program
symbol id plus its Program stamp, or a private store index plus its Checker
stamp. Constructors and fields are private to the symbol access module; there
is no conversion from a private index to `tsr_binder::SymbolId`. A node link is
`Option<SymbolRef>`: `None` means uncomputed, while unknown is an actual private
symbol. Unresolved and merged targets are other private records, not additional
meanings of `None`.

Each stamp owns a distinct `Arc` allocation with a nonzero payload. Equality
uses `Arc::ptr_eq` and hashing uses its allocation identity; deriving equality
from the equal payload would collapse different domains. The Program creates
its stamp once with its bound store and every Checker borrows that same store.
The Checker creates its private stamp once. Moving either owner does not change
its stamp. A retained handle keeps the allocation alive after its store drops,
so another owner cannot reuse that allocation address while the handle exists.
The stamp owns no AST or symbol records and extends none of their lifetimes.
Access still requires a live Checker with the matching borrowed Program.

Bound handles are accepted by both Checkers on the same Program. Bound handles
from another Program and private handles from another Checker are rejected
before indexing or returning a type. Treat a foreign domain as an ownership
error, not as a missing-name result. No global counter, semantic cache, unsafe
branding, shared mutable store, or new dependency is required. Keep `Clone`
explicit: this owned boundary handle is not `Copy`.

The [executable ownership layout](../../crates/tsr-checker/tests/symbol_domain_contract.rs)
uses actual binder SymbolIds and checks these domain rules, pointer-based
hashing, owner-drop behavior, moving a Checker, unresolved parent paths and
bound origins of private clones. It now exercises the production store, including
cloning, redirects, immutable Program records and foreign-edge rejection. Full name/alias resolution and
module preparation remain separate tasks.

The private store contains a contiguous record vector, a full-path unresolved
table and a merged-symbol redirect table. Its minimum record carries flags,
CheckFlags, `Cow<str>` name, declaration ids, value declaration, parent and
export-symbol handles, optional member/export maps with `SymbolRef` values,
and explicit origin metadata where the native operation requires it. Own names
for synthetic paths; borrow bound names while the Program is alive. Origin
may itself be private when a private symbol is cloned. Preserve absence of a
table until its native initialization point; an empty table is not a completed
member image.

Unknown is a private Property/Transient symbol named `unknown`. Unresolved
records are TypeAlias/Transient with CheckFlagsUnresolved, full-path parent
handles and declared-type linkage to the private unresolved type. The unresolved
table key is the full textual path within this Checker; it excludes source
scope, AST identity, generic arguments and the current substitution frame.
Each node still does its own first lookup and diagnostic before sharing that
symbol. An empty/missing name returns unknown through the native path.

Classify private storage by **SymbolFlagsTransient**, not nonzero CheckFlags.
Native `newSymbol` at 14070 sets Transient; `cloneSymbol` at 14343 also uses it
and leaves CheckFlags zero. A clone owns separate member/export maps whose
existing symbol values remain shared handles. Its declaration sequence must
be copied before append (native caps the shared slice to force reallocation).
Copy exactly the native clone fields: CheckFlags and ExportSymbol are not
automatically inherited by `cloneSymbol`; origin metadata does not change that.
`recordMergedSymbol` at 14372 redirects source to result inside this Checker;
it does not mutate a bound symbol or rewrite the Program's binder redirects.

During an export-table merge, an actually merged private export gets the
private merged module as its raw parent. A source-only export retains its
original raw parent. `getParentOfSymbol` applies late-bound and merged redirects
when reading that parent: after a bidirectional merge it can return the merged
module even when the raw parent remains the source. Unidirectional merges do
not publish a redirect for the source; cloning still redirects the original
target. Do not normalize these two parent channels into one eagerly rewritten
field or replace declaration merging with a property-type union.

## Access and consumer migration

Put stamp validation, private indexing and merged/parent access behind one
symbol access module. Reads return flags, declaration ids and cloned handles,
or a short-lived immutable view. Drop any view before recursive `&mut self`
work, then publish by handle, following [ADR-0013](../adr/0013-checker-memoisation.md).
The private store may keep compact local indexes internally, but they cannot
escape its encapsulated records/tables. Other Checker methods and type owners
receive tagged handles; making a bare local index crate-visible would bypass
the very domain check this layout establishes.

| Existing boundary | Required migration before private results reach it |
|---|---|
| `symbols.rs::get_type_of_symbol`, `get_symbol_flags`, alias/export/module helpers | Add handle-taking internal entry points. Keep existing bound-id APIs as compatibility wrappers that construct a handle from this Checker's Program. Resolve flags, declarations, merged targets and parents through the accessor. |
| `declared.rs::get_declared_type_of_symbol` and type-reference selection | Accept private unknown/unresolved/merged results and keep current instantiation, written-name and alias-frame work after static selection. Key node completion by the entire TypeReference node, not its name child. |
| `members.rs` owner traversal, `get_property_of_type[_ex]`, optional/readonly queries | Return and consume handles, including private merged/instantiated properties. Resolve the concrete receiver and arguments after owner selection under `.4.1.5`/`.4.2`; never return `None` merely because a property is private. |
| `signatures.rs::get_signatures_of_symbol`, class construct signatures | Read the private symbol's declarations and typed member edges. Signature objects currently carry declaration/TypeIds, not a symbol field; migrate their owner keys and readers, not an invented field. |
| `TypeData::Named.members`, `Anonymous.symbol`, Union/Intersection alias symbol and EnumLiteral owner/member | Replace reachable owner fields with `SymbolRef` and update factories/consumers. TypeData/TypeStore are nongeneric, so borrowed AST/store references cannot solve this channel. Preserve complete interning identity, constituent order and freshness; handle equality includes domain. No fake Program index or duplicate fallback side table. |
| Checker `symbol_types`, `declared_types`, `this_types`, signature cache, `resolutions` and instantiation/alias target metadata | Migrate keys/values when their native operations can select private symbols; resolution stack keys must preserve property kind as well as symbol identity. Include `type_reference_targets` and deferred alias/mapper ownership channels in the call-site audit. |
| Binder declaration lookup, bound assignment scans and bound declaration-check sets | Remain Program-id APIs where the operation really addresses a binder declaration. A private clone explicitly reaches its origin/declarations through the accessor; no unchecked demotion. Do not mechanically widen every SymbolId table. |

Bound members/exports use `SymbolTableField`, preserving native absent versus
initialized-empty state. Scope tables retain `SymbolTable`. Reads preserve
absence; explicit initialization and insertion publish a table, and removal or
clear retain its presence. Bound-to-private clones copy that state with independent
tables. The [presence evidence](checker-symbol-table-presence.md) records native
controls, public API compatibility, complete preservation and measured overhead.
Native prototype/signature contents and disjoint internal keys remain
`tsr-1yb.7.7.1.1.1` / `.1.1.1.1`; metadata agreement
does not certify those complete images. Storage and table-fidelity tasks remain
open until their content/cost gates pass. Do not infer absence from emptiness or
use has_member_table/has_export_table as a completed-member predicate.

Owner changes to public TypeData variants/factories need a repository-wide
caller audit and coordinated edits. Bound-only compatibility APIs do not
provide a general private-symbol query. A new handle getter cannot be enabled
while a downstream owner still assumes all results belong to the bound store.

Keep alias-target links separate from node links. Native `resolveAlias` at
16266 pushes the AliasTarget resolution property, resolves pure aliases through
`resolveIndirectionAlias`, propagates type-only declaration metadata, publishes
target-or-unknown, and can replace it with unknown on a failed pop while
reporting the cycle. `tryResolveAlias` can decline an active unresolved cycle.
This is not equivalent to one per-symbol resolving bit or memoizing Rust's
current dispatch. Restore the stack/property protocol when
porting transitive alias completion. The alias node getter's nonpublishing
fallback remains nonpublishing.

Current-source correction, `e744714c`: the historical nonrecursive rationale in
`symbols.rs` is stale. The external import-equals arm at1298 calls
`resolve_alias` again, and the identifier arm at1243 can force an alias through
`get_type_of_symbol`. This does not prove faithful native transitive publication;
it prevents treating the old zero-frame observation as a current safety proof.
[Fresh cycle controls](checker-alias-target-cycle.json) preserve a failed
`node10` setup (nativeTS5108) before qualifying a NodeNext import-equals/export=
cycle in default and single modes. Native reports fourTS2303 diagnostics; TSR
reports those four plus two falseTS1203 and oneTS2708. The per-file CommonJS
format gap is `tsr-2zk.6.19`; alias unknown/meaning and push/pop/type-only
publication remain `tsr-1yb.7.7.3`. Termination and matching cycle errors alone
do not qualify memoization or query-before/after-check behavior. No runtime edit
or broader corpus/speed claim follows from these four valid CLI children.

Construction order is bind/freeze Program inputs, create the private Checker
store, run native preparation/merge behavior, then permit supported completed
node queries. `tsr-6.49` and `.3.2` own that preparation and worker-affinity
contract. This design does not introduce a guessed version counter or clear
completed entries to compensate for an unported late merge. Written aliases,
flow nodes, type arguments and substitution frames remain separate channels.

One stamp per Program, one per Checker, and handle clone/refcount work are real costs.
Count handle construction/clones, lookup worker executions and retained map/
record capacities on current source before choosing a compact production
encoding. The layout proves safe ownership, not profitable reuse. `.7.7` must
measure the integrated change against its unchanged whole-project gates.

## Current alias-target qualification

The 2026-10-08 audit uses TSR `59f6ce22` and pinned native `5b1047d1`.
Main already memoizes alias targets; the earlier "intentionally unmemoized"
description in the original contract and ticket is historical. Its separate
circularity walk still stops at 64 hops, while native `resolveAlias`
(`checker.go:16266`) marks every participating resolution frame as failed.
A natural 70-alias cycle, with one leading alias outside the cycle, reports
70 TS2303 diagnostics in native default and single modes and none in current
TSR. The two-alias cycle and 70-link acyclic controls agree already.

A private stack/indirection candidate produces byte-identical native CLI stdout
for all six of those default/single controls and passes two new public controls
plus all 37 cross-file alias tests. Its checker-library gate is **187 passed,
two failed**, so the candidate is not integrated. These are debug correctness
runs, not ordinary-release timing or whole-corpus preservation. Source-bound
executables, native observations, original failed setups and immutable candidate
snapshots are retained at
`target/native-optimization-goal/alias-stack-current-59f/`.

The module-copy test first fails on incorrect namespace-meaning admission;
natural native controls also identify the private-symbol boundary that a full
repair must preserve. Native `resolveESModuleSymbol` / `cloneTypeAsModuleType`
(`checker.go:15568`, `:15721`) returns a fresh private symbol for each originating
namespace import, preserving raw declarations, tables, flags and parent while
publishing its completed module value separately. Natural native controls prove
that `Head` and `Twin` remain distinct and differ from the bound class. When the
class has namespace meaning, `Linked` and `Tail` retain `Head`'s clone and its
filtered members; without namespace meaning their targets are native unknown.
TSR's bound-only alias target loses this private identity. The existing
`symbol_access::clone_symbol` also records a merge redirect; native's module-clone
writer does not redirect its source, so that writer is not a substitute.
`tsr-1yb.7.7.2.1` owns this concrete consumer migration and blocks `.7.7.3`.

The other failed test expects the immediate `export=` alias from `resolve_alias`.
Native has separate `getImmediateAliasedSymbol` and flattened `resolveAlias`
operations: a natural `a0 -> a1 -> Target` control proves the former returns
`a1` and the latter `Target`. Its first flattened query executes two alias
workers; repeated queries execute none. A type-only re-export control retains
the actual ExportSpecifier declaration as a backlink. Four seeded native
stack controls separately verify nonmarking `tryResolveAlias`, all-participant
failure, publication before the equality check, `resolutionStart`, and type-only
backlink precedence. Seeded controls do not prove natural Rust query ordering.

Before retaining the full port, carry private targets through the existing
`SymbolRef` domain, preserve native unknown separately from unsupported work,
wire nonmarking try and all represented publication facts, and migrate immediate
consumers deliberately. Keep `alias_resolving`'s active-worker boundary for the
current base-symbol cache. Complete diagnostic/related output, full oracle,
previously RIGHT preservation, private-checker isolation and actual worker/copy
counts remain required. No broader reuse or speed claim follows from these
focused controls.

### Module-clone record and owner continuation

At frozen TSR `85f9c4cb` / native `5b1047d1`, the private no-redirect
record-copy operation preserves declaration order, value declaration, raw
parent, flags and independent member/export tables. Two characterization
controls fail under the existing redirecting clone; the new operation passes
191 library and 16 ownership controls. Reintroducing the redirect fails both
new controls, and exact restoration passes again. This qualifies only the
record-copy phase, not native module-value or alias-target publication.

Ten natural native observations cover five valid cases in cold and checked-first
orders. Each namespace import owns a fresh copied symbol and its completed
anonymous value, with raw export target and originating ImportDeclaration;
repeated and after-check reads preserve identity without alias/copy workers.
Both call and construct signature lists are empty. The selected synthetic
namespace value has zero index infos in the indexed-class case, while a direct
clone of that natural class value preserves its one index. The import wrapper
and clone operation therefore need separate controls. Ordinary pinned CLI
rejects `esModuleInterop=false` with TS5108; that attempted setup is retained
and excluded from the ten observations.

A parsed Rust owner control remains red both before and after checking: `Head`'s
anonymous owner is the raw class's `SymbolId(1)`. Four ordinary native/debug TSR
CLI children on valid indexed input expose a second consequence: native emits
TS7053, while TSR forwards the raw class index and emits TS2322. These are
correctness observations, not performance or complete related-diagnostic gates.

The private migration now carries `SymbolRef` in `Anonymous.symbol`, lifts twelve
bound construction sites and gives module-copy values their fresh private owner.
That initial migration is uncompiled: 107 diagnostics across 21 consumer files,
after 119 at the field cutover. The signature continuation below reaches 98
across 20 files; neither count is a work ceiling. Bound/anonymous combined
patterns, signatures, declarations, member
reads, printing, flow and cache identities must retain the real handle; do not
collapse private owners to their source to satisfy those types. No runtime
change or speed gain is retained. `tsr-1yb.7.7.2.1` remains in progress.
The [lossless progress receipt](checker-module-clone-progress.json) includes
native control sources, worker observations, the tested private writer patch,
failed gates and the parsed red control. Local continuation sources live under
`target/native-optimization-goal/module-clone-current-85f/`.

### Signature-owner continuation

The frozen `85f9c4cb` private signature slice now accepts actual `SymbolRef`
owners. Bound entry points remain compatibility wrappers; the worker validates
before work and reads the selected record's ordered declarations. It retains
the existing declaration/mapper signature keys and shape-versus-return demand
split. No symbol-origin translation or new signature cache is introduced.
Five Anonymous callable consumers use the existing type-signature accessor;
cached call returns stay deferred until candidate selection, and completed
empty lists remain authoritative.

Four direct pinned-native observations distinguish these operations. A fresh
copied function retains two raw overload signatures with the original
signature/declaration identities, but its completed module value has no call
or construct signatures. Appending a naturally parsed boolean declaration to
the copy adds a third raw signature while the source remains two and the module
value remains noncallable. These are direct operation controls, not alias-worker
counts or ordinary CLI measurements.

The private reader passes four ownership/overload tests. A compiling mutant
that follows the source edge loses the copy-only declaration (two signatures
instead of three); exact restoration passes 195 library, 16 ownership and 135
affected integration tests. One existing test remains ignored, and the known
red parsed import-owner regression is explicitly filtered from this isolated
slice. The initial exact filter ran zero tests; its qualified rerun runs one.
Missing-method API-red runs are separate from the behavioral mutation proof.
Strict library Clippy still fails on the one unused module-copy writer in this
slice; that writer is consumed by the broader uncompiled migration.

The broader migration preserves actual owners through propagated and
instantiated signatures, alias remints and module-value construction. At that stage, its
library check had 98 diagnostics across 20 consumer files, down from 107.
Merged owner reads, declarations, members, class signatures, naming and native
cache/publication keys still require migration. This is private progress, with
no canonical runtime, full-corpus qualification or speed gain. The existing
[progress receipt](checker-module-clone-progress.json) carries the exact native
sources, both Rust patches, source/binary hashes, failures and final logs.

### Raw-owner continuation

The same frozen `85f9c4cb` continuation now reads actual private flags,
ordered declarations and raw export keys at ten anonymous-owner consumer
sites. The existing static-member and private-name completeness predicates
use validated views, and the `keyof` reader retains its declaration/name sort.
Export-name iteration uses the selected table without an intermediate
collection; it does not imply completed native members. Bound enum-member
owners are lifted into the validated domain for exact comparison with the
receiver, so a fresh private enum copy is not confused with its source.

Four direct pinned-native observations confirm independent raw/copy/twin
export tables, copied const-enum flags and enum-member parent identity. Adding
an export only to the copy leaves the source and twin unchanged. These are
native operation controls, not ordinary CLI work counts or timing samples.
Two bound-path characterizations pass before implementation. The private
reader controls mutate only the copy's declarations; compiling origin-fallback
and empty-private-export mutants fail two and one tests respectively. Both
mutations restore exact bytes before final qualification.

The final frozen slice passes 446 debug tests: 199 library, 16 ownership and
231 integration, with one existing ignore and the known red parsed owner test
explicitly filtered. Source manifests remain unchanged during verification.
Strict library Clippy still fails on one unused module-copy writer. The broader
owner cutover now has 85 compiler diagnostics across 16 consumer files, down
from 98; it is not an executable checker. The existing progress receipt
preserves eight-file slice and seventeen-file cutover patches, native sources,
binary/source hashes and all gate logs. No canonical runtime, full-corpus or
performance gain is claimed.

The next boundary is merged-owner publication and its cache consumers.
Native `getMergedSymbol` is one redirect, and native `mergeSymbol` makes a
private copy of a nontransient target. Rust's existing Binder mutates a bound
target and follows a bounded redirect chain. Neither an origin fallback nor
that legacy bound normalization establishes native private identity. Continue
this boundary under `tsr-1yb.7.7.2` and `tsr-1yb.33.1` before completing the
parsed module-owner and alias-stack gates.

### Symbol-key continuation on current main

The prior writer/signature/raw-reader patches replay onto frozen main
`1cea3449` without conflicts. Its separate baseline target passes 194 checker
library tests. The new private key slice changes the existing value-type and
declared-type maps and symbol resolution frames to `SymbolRef`, then lifts
existing bound callers through the same Checker-owned accessors. Undefined's
seed and later queries share one `CheckerSymbols` instance. Signature and
base-constraint keys remain separate entities; no duplicate cache or origin
fallback is introduced. This is an ownership prerequisite, not measured reuse.

Two direct native observations at `5b1047d1`, cold and checked-first, call the
real `cloneTypeAsModuleType`, `cloneSymbol`, `pushTypeResolution`,
`findResolutionCycleStartIndex`, `popTypeResolution` and
`typeResolutionHasProperty`. Source/head/twin declared-type frames push
true/true/true; repeating head returns false, leaves depth three and pops
false/false/true. A different property kind does not close that cycle. Native
record clones inherit neither the source's already-computed value type nor its
declared type. The Rust controls use real parsed owners and ordinary bound
getters to publish those source types, then query the actual publication reader
for fresh private copies. A compiling origin-key mutant fails the latter
control; its exact source bytes are restored before final verification.

The source-qualified slice passes 453 debug tests: 206 library, 16 ownership
and 231 integration, with one existing ignore and the known parsed import-owner
red explicitly filtered. Its separate refreshed run still fails because the
namespace-import value carries the raw owner. The separate targets and unchanged 1,053-file
manifests tie those results to the actual slice executable. Strict library
Clippy reports six errors: five also occur on the exact current-main baseline,
and the additional unused module-copy writer remains unintegrated. No lint is
suppressed. The [progress receipt](checker-module-clone-progress.json) preserves
26-file slice and 32-file full-cutover patches, verified lossless replays, native
source/output and binary identities, API/compiler failures and the semantic
mutation failure. Historical `85f9c4cb` fields remain unchanged.

Twelve further Anonymous-owner consumers now carry actual flags, cache/frame
keys, raw parent/name or exact identity in the full private cutover. They are
not qualified by the passing slice. Its compiler gate moves from 85 errors at
replay, through 89 and 72, to 69 across twelve files; these are compiler
observations, not a count of remaining work. The full checker still does not
compile. Type/declared/member/class/naming consumers, other owner caches and
merged publication remain unfinished.

The existing publication predicate also excludes `intrinsics.error`, while
native tests whether a link is nonnil. The port still conflates unsupported work
and some computed failures at that boundary. This key migration does not
certify all error/completion states or authorize broader reuse; preserve that
boundary under `tsr-1yb.7.7.2` and `tsr-1yb.7.7.3`. The unfiltered parsed owner,
alias-stack, full previously-RIGHT/diagnostic/performed-work and ordinary CLI
performance gates still follow. No canonical runtime or speed win is retained.

### Constructor and naming continuation on current main

Frozen main `13da1cf0` has the same Rust runtime as `1cea3449`. The next private
migration retains actual `SymbolRef` keys in the existing base-constructor
cache and `ResolvedBaseConstructorType` frames, with bound compatibility
entries. Module/ambient classification and module specifiers read the selected
record. Qualification and shadowing retain the selected target; scope-table
entries and their existing bound merge/alias lookup remain unchanged. Active
parameter-only signature eligibility reads the actual value-cache key and
compares declaration ownership by lifted identity. It does not accept a clone
merely because it shares a declaration. No new semantic result cache is added.

Four pinned `5b1047d` native parsed-class observations cover cold and
checked-first orders. `getAccessibleSymbolChain` returns `[C]` for the source
and no chain for either module copy; `needsQualification` is false/true/true.
Native fresh declared class types have independent base-constructor slots:
only the source is initially published, an active head returns `errorType`
and pops false, and later head/twin queries complete. Rust's two new controls
exercise real parsed owners, private record copying, qualification and actual
constructor cache/frame operations. Compiling origin-substitution mutants
fail both controls before exact restoration. Native's slots are type-owned;
this port still uses its existing symbol-owned constructor links. The private
declared-type factory and complete private accessibility/alias chains remain
unported, so these direct controls do not qualify those natural consumers.

The compilable key slice passes the checker package with **1,569 tests**,
including 208 library tests, across 115 result blocks; three existing ignores
remain and the known parsed import-owner red is explicitly filtered. A separate
correctly targeted `cross_file_aliases` run fails that red: head and raw value
owners are still `SymbolId(1)`. An earlier mistaken target executed zero tests
and is retained as a failed verification attempt, not passing evidence. All
1,053 input hashes remain unchanged during the final run; all 114 executed
test binaries carry this source root and have recorded hashes.

The full payload cutover additionally migrates negative `instanceof` and
`Symbol.hasInstance` declaration scans, declaration-position ordering, module
flag guards and anonymous constructor dispatch. Its compiler observations move
**69 -> 63 -> 55 -> 52**, ending with 52 type mismatches across eleven files.
That source still does not compile and is not qualified by the passing key
slice. Strict all-target Clippy remains red: library six versus baseline five;
library-test five on both. Three previously missed test import-order errors
were corrected without suppression. Both snapshot formatter checks pass.

The [receipt](checker-module-clone-progress.json)'s
`constructor_naming_continuation` preserves lossless 26-file key and 33-file
full-cutover patches, complete manifests, native changed sources, actual
executable identities and raw passing/failing outputs. Historical receipt
fields remain unchanged. Type/declared/member/class/signature and naming
consumers, checker merged-table publication and the unsupported/computed-error
boundary remain under `tsr-1yb.7.7.2` / `tsr-1yb.7.7.2.1` / `tsr-1yb.7.7.3`.
The six goal tickets remain unfinished. Canonical runtime, full previously-RIGHT
corpus, actual whole-project work and verified TSR/native median <=0.50 are
unchanged and unproved by this checkpoint.

### Declared class and reference owner continuation

Frozen main `67440ffd` still has the `1cea3449` Rust runtime. The new private
continuation migrates the actual class owner through local parameter and JSDoc
readers and anonymous-class naming. Copies retain their declaration nodes and
those parameters' Program-owned symbols; the copied class itself retains its
private identity. Bound compatibility entries use validated handles.

Pinned native `5b1047d` direct parsed-class controls now exercise
`getDeclaredTypeOfClassOrInterface`,
`appendLocalTypeParametersOfClassOrInterfaceOrTypeAlias` and
`createTypeReferenceEx`. Cold and checked-first orders both pass all ten
identity/reuse checks, with zero diagnostics: parameter types are shared,
source/head/twin declared types and references remain distinct, references
retain the selected owner, repeated arguments reuse the reference and reversed
arguments stay distinct. The complete native source replay contains thirteen
changed Go files, including eleven inherited control/instrumentation files;
5,005 input hashes are unchanged through the qualified build and run.

The full Rust draft changes `Named.members`, the existing instantiation keys
and reference-target metadata to `SymbolRef`. Its selected class factory uses
the existing declared-type link. A class-reference worker and common object
reference publisher retain actual owners and ordered arguments in the existing
stores; bound class reference calls delegate to that worker. Fifty-five bound
writer arguments and four variable cache keys are lifted at their current
writers. Alias preparation and many consumers remain bound-only. Native's early
`this`/outer-parameter class publication is not implemented by this named-type
factory, and the new private factory test has **not executed**.

The separately compilable reader/key draft passes **1,570 checker-package
tests**, including 209 library tests, with three existing ignores and one known
import-owner red filtered. That red is separately executed and still fails:
head and raw anonymous owners are both `SymbolId(1)`. All 1,053 inputs remain
unchanged during the final package run; 114 actual executables have source-root
and hash receipts. Strict Clippy remains six library errors versus the inherited
runtime-equivalent baseline's five, and five library-test errors; the new
readers add no reported lint errors. Formatter checks pass.

Expanding the owner fields exposes additional consumer migrations: the full
library reports **267 compiler errors across 37 files**, after observations of
177, 317 and 271 during field/writer migration. These are unfinished type/API
boundaries, not a speed result or a ceiling on the required work. Private
symbol/type dispatch, members, bases, signatures, relations, flow, naming,
alias/enum/`this` owners and native publication still need completion. The
[receipt](checker-module-clone-progress.json)'s
`declared_class_reference_continuation` preserves the lossless 26-file reader
and 36-file full drafts, native replay, full compiler errors and raw gates.
Historical qualified sources/fields remain immutable. No canonical runtime,
full previously-RIGHT corpus or speed gain is claimed; all six goal tickets
remain unfinished and the equivalent complete-work median <=0.50 target remains
unmet and unverified.

### Reference, polymorphic this and member-owner continuation

Frozen main `4a0ff849` continues the immutable `67440ffd` draft. The pinned
native `5b1047d` `getDeclaredTypeOfClassOrInterface` writer publishes a class's
own `thisType`, constrained by its own declared type. Actual parsed `Box<T, U>`
module copies retain the same parameter declaration types but distinct declared,
reference and polymorphic-this types. Two cold/check-first native observations
pass all 34 ownership, reuse and ordered receiver-substitution assertions with
zero diagnostics. These direct workers do not certify natural imports.

The private full draft now keys the existing `this_types` table by actual
`SymbolRef`; bound expression, explicit-this and type-node writers share its
factory. Declared class publication mints this after publishing the declared
link. `type_parameter_constraint` follows the selected class owner. Full native
outer-parameter publication and early interface this policy remain unported.
The legacy interface declaration-node table supplies a this type only when its
bound declaration owner equals the selected symbol; copied declarations cannot
supply the original owner's this type to a private copy.

Reference-target queries retain actual handles, and generic receiver workers
read selected declarations and parameters. Existing receiver/declared/this-arg
memo keys, alias-frame admission, publication marks and provisional refusal
rules remain in place. Generic heritage cycle paths use actual owner handles.
The existing base-cache key now retains the selected owner and refusal policy;
its declaration syntax still calls the bound heritage entity resolver and keeps
bound result symbols. Private alias/heritage target resolution remains explicitly
unfinished in `tsr-1yb.7.7.2.1` and `tsr-1yb.7.7.3`.

A raw-member iterator preserves each table edge's actual handle, including a
private edge that differs from its source and sibling copy. It neither resolves
aliases nor publishes member completion; absent/empty presence remains separate.
Property enumeration and completeness walks read those edges and retain actual
owner cycle identities. Another 53 compiler-identified raw metadata reads across
18 files use selected views. No origin projection or additional semantic store
is introduced. Builder/substitution work and copies remain under the existing
`tsr-1yb.11` attribution; this identity migration claims no saved work or speed.

The compilable reader slice passes all 209 library tests, including the
strengthened member-edge ownership control. Its separately executed parsed
import-owner test still fails Head/raw `SymbolId(1)`. The full 40-file owner
draft remains uncompiled: 204 library errors (183 type mismatches, 21 ownership
moves), down from the inherited 267. Its class/reference/this/substitution test
has not executed. Strict reader Clippy retains six library errors and five
library-test errors; the earlier runtime-equivalent baseline had five library
errors. Both formatter checks pass. The final 1,053 inputs in each Rust slice
and 5,005 Go inputs remain stable around qualification, and binaries identify
their new source roots. Exact-byte replay verifies 26/40 Rust and 13 Go changed
files. The [lossless archive](checker-module-clone-progress.json) preserves all
older records plus `reference_this_member_continuation`, including failed API,
iterator-lifetime and patch-format checks. No main runtime change, complete
corpus gate or equivalent-work <=0.50 speed certificate follows.

### Alias-body, relation and declaration-walk owner continuation

Frozen main `55dd4941` continues the preceding full-owner archive. Native
`5b1047d` `getTypeAliasInstantiation` (`checker.go:23641`) selects the actual
symbol's links before looking up ordered arguments and alias context. The
existing Rust `alias_body_evaluations` map now retains `SymbolRef` rather than
`SymbolId`; its existing body/conditional evaluation and result publication
remain unchanged. This is a domain migration, not a new cache or expanded
reuse policy. The broader native mapper and alias-key contract remains under
`tsr-1yb.4.1.2`; the Rust alias-body slice still lacks the full native factory.

Bound entry points lift their owner explicitly and share selected-owner workers
for alias bodies, conditional evaluation, branch capture, conditional declaration
discovery, closed literal unions and NoInfer admission. Selected declarations
come from the actual view; their syntax nodes and parameter binding symbols stay
Binder-owned. The enclosing result-alias presentation identity stays in its
existing declaration domain. The inner conditional-reference entity resolver
remains bound, tracked by `tsr-1yb.7.7.2.1`/`tsr-1yb.7.7.3`; no origin projection
or conversion of an unsupported private target to native unknown is added.

Full-draft reference relation pairs and `getRecursionIdentity` readers now carry
actual handles. The recursion comparison borrows that identity through nested
intersections. Global identity comparisons explicitly lift bound globals;
selected merged-symbol reads use `CheckerSymbols`. Raw keyof enumeration reads
actual member edges and declaration order. Array-base and private-name walks
track selected owners while their existing heritage syntax resolver supplies
bound declaration targets. These traversals retain their previous cycle and
refusal rules; they neither publish completed member fields nor introduce
another semantic store. Symbol/type, naming, member, signature, variance and
private heritage dispatch are still incomplete.

The compilable reader slice passes all 209 library tests. The strengthened
literal-union control checks that a selected-owner alias-body query reuses the
already evaluated union without changing the existing cache; its API-red check
failed for the missing method before implementation. The parsed import-owner
test still fails Head/raw `SymbolId(1)`. Full compilation is still red: 114
errors across 25 files, comprising 108 type mismatches and six ownership moves,
versus the inherited 204 errors. The private class/reference/this/mapper test
has not executed. Compiler counts are not semantic coverage or speed scores.

Both formatter checks pass. Strict reader Clippy retains the inherited six
library/five library-test errors after correcting the newly introduced wrapper
semicolon lint. Each Rust slice has 1,053 stable qualification inputs and the
reader executable identifies this source root. Exact-byte delta replay covers
three reader files and 35 full-draft files, with 684 Rust files equal per replay;
all inherited preimages are checked against canonical archived sources or the
runtime base. The [lossless archive](checker-module-clone-progress.json) adds
`alias_relation_owner_continuation` without altering historical fields. Failed
checks and their corrections remain recorded. No native execution is newly
qualified here; earlier native controls retain their original source labels.
There is no main runtime change, full corpus gate or equivalent-work speed win.
Actual expensive-worker attribution remains `tsr-1yb.11`, and the release target
remains verified TSR/pinned-tsgo median wall ratio <=0.50.

### Selected metadata and heritage consumer continuation

Frozen main `27ec35b7` continues the actual owner migration under
`tsr-1yb.7.7.2.1`. Selected flags, declarations, reference names and raw member/
export presence now come from `CheckerSymbols` views. Generic-alias cycle keys,
narrowing completeness, class-extends and heritage traversal retain actual
handles. Bound declaration entry points lift explicitly; the existing heritage
syntax resolver still supplies bound targets. Its private alias/entity dispatch
remains unfinished under `tsr-1yb.7.7.3`. These are existing cache and traversal
domain changes, preserving their active-alias refusal, cycle and depth policies.

Native `5b1047d` `hasBaseType`/`getTargetType` (`checker.go:19551`) compares actual
targets, not names or overlapping declarations. The reader's existing class
member-order control now parses `Shape extends Base`, clones each actual symbol,
and proves the private Shape follows bound Base but does not derive from the
distinct private Base clone. The test first failed to compile for the missing
selected heritage API; after migrating the existing base-cache worker/key and
walks, all 209 library tests pass. This is a direct private-handle control, not a
new native execution or full module-clone certificate.

The full draft remains uncompiled: 70 type mismatches across 19 files, down from
114 after intermediate 98/86/71-error checks. The separate parsed module-owner
test still fails Head/raw `SymbolId(1)`. Both fmt checks pass. Strict reader
Clippy returns to the inherited six library/five test errors after fixing two
new lints. Borrowed declaration iteration removes the narrowing-presence walk's
temporary vector; no measured performance benefit is claimed. Short owner
clones end immutable borrows before mutable work, without copying whole type
payloads. Each slice has 1,053 stable inputs and the reader binary identifies its
source root. Canonical reconstruction from runtime plus inherited archives
checks all 684 Rust files per slice before and after the exact three/24-file
deltas. All 33 historical fields stay identical; the
[archive](checker-module-clone-progress.json) adds
`selected_metadata_heritage_continuation`, including failed checks/corrections.

Selected symbol/type/value, general alias/reference factory, member/static/
index/signature and naming dispatch are still required. Native
`getVariancesWorker`/`createMarkerType` (`relater.go`) store variance on the actual
symbol's links and instantiate that owner's declared type. Their source was
read here; the actual factory must migrate before variance reuse can retain
private owners. Full factory/this/mapper, previously-RIGHT corpus and equivalent
complete-work median <=0.50 gates remain unverified. No canonical runtime or
speed win follows from this checkpoint.

### Interface signature owner integration

The production continuation at frozen main `f1ba6b6b` changes the existing
`interface_signatures` key and visiting stack to `(SymbolRef, SignatureKind)`
and `SymbolRef`, respectively. The selected worker reads the actual merged
owner's declarations through `CheckerSymbols::view`. The bound entry preserves
`binder.merged_symbol` before lifting the handle, an explicit Program boundary
for current named-type and heritage callers;
there is no private-to-origin conversion and no second result store.
This is a prerequisite under `tsr-1yb.7.7.2.1`, not completion of its private
type/factory and natural import-owner migration.

The native pin is `5b1047d`. `resolveDeclaredMembers`
(`internal/checker/checker.go:19612`) obtains declared call/construct signatures
from the selected owner's `__call`/`__new` member symbols.
`getSignaturesOfStructuredType` (18963) and `resolveObjectTypeMembers` (19106)
read and publish the structured type's fields. TSR retains its existing
declaration walk and completed-result policy here: `Some`, including empty,
publishes; an unreadable declaration, failed base or active cycle returns
`None` and does not publish. Declaration/mapper signature identity and
per-base instantiation before receiver substitution are unchanged. Actual
native member-table construction and provisional/reset publication remain
under `tsr-1yb.33.1`; broader construction/reuse remains under
`tsr-1yb.4.2.1`. This key migration does not certify those protocols.

Fresh native controls make that distinction observable. Appending a second
interface declaration to one private owner, without updating its member slot,
returns counts `[1,1,1,1,1]`, contradicting the initial setup expectation.
The corrected direct record control also constructs that owner's private
`__call`/`__new` slot before its first read. It returns `[1,2,1,2,1]` for twin,
copy, original, copy, twin in all four call/construct and cold/checked-first
combinations. Distinct declared types, repeat identity, declaration signature
identity, opposite-kind emptiness and an unredirected original all hold.
Both runs are byte identical; all 55,105 native inputs stay stable. This is
a fully formed private-record control, not a natural module-clone producer.
The failed compile/setup attempts are preserved rather than counted as passes.

The strengthened existing Rust unreadable-interface control varies strict
null checking, call/construct kind, inherited and unreadable declarations,
and two independent private clones. A compiling origin-substitution mutant
fails `Some(0)` versus `Some(1)`, so the control detects selecting the source
instead of the actual owner. Local review also exposed a dropped Program
merge redirect in the initial wrapper: a compiled two-file interface fixture
returned `Some(1)` instead of `Some(2)`. Restoring that redirect at the bound
boundary passes the same test for call and construct kinds through stale and
merged entry points. The final production package passes 1,556 tests and
the workspace passes 3,241, with three and 19 existing ignores. Current
unfiltered expected-driven outputs are byte identical to frozen `f1ba6b6b`:
552,533 type rows and 12,238 diagnostic cases, with zero previously RIGHT
losses. These outputs are a preservation gate, not fresh full-native parity.
An initial present-but-empty `TSR_FILTER` excluded all diagnostic cases; those
zero-case attempts are invalid and retained. Qualified diagnostic runs unset
the variable. Formatting passes. Strict workspace Clippy reproduces seven
inherited errors on both baseline and final candidate: five checker errors
(in both library and test builds) and two `tsr-dts` accessibility-test errors.
There are no new lint errors in the owned paths. The earlier five-error
workspace result did not reach those two test errors; it remains historical.

The wider private reader passes 209 library tests; the complete owner draft
still fails compilation with 69 `E0308` errors, down from 70. Its full
factory/mapper and natural alias controls remain unexecuted. No performance,
allocation, worker reduction or coverage improvement is claimed. The
equivalent complete-work median TSR/native <=0.50 requirement remains open.
[Evidence](checker-interface-signature-owner.json) records exact production
inputs/binaries, native control source, failed controls, private two-file delta
replays and all remaining compiler diagnostics; the preceding historical
module-clone archive remains unchanged.

### Template alias owner integration

Frozen main `835ef559` plus the two Rust overlays in
[the evidence](checker-template-alias-owner.json) moves the existing template
alias worker and its active set to actual `SymbolRef` owners. Bound callers
lift their existing input at the Program boundary and use the same worker.
The selected view supplies flags and declarations; it never substitutes a
clone's origin. Shared syntax parameter symbols still key the temporary
argument bindings, and each evaluation restores its own binding frame.

At native `5b1047d`, `getTypeAliasInstantiation` reads the selected owner's
declared type and alias links before mapping its ordered parameters.
`getTypeFromTemplateTypeNode` and `getTemplateLiteralType` retain structural
template/literal reuse. Fresh native cold and checked-first controls, in both
query orders, confirm distinct alias links and a still-uncomputed private
owner while its original and sibling are forced under its `DeclaredType`
frame. Equal completed string literals share identity. That does not authorize
sharing alias completion state.

Rust's existing template-only active set remains an evaluation guard, **not**
native `DeclaredType` or `AliasTarget` publication. Unsupported body/arity
returns do not enter it; reentry leaves the outer entry intact; an evaluated
result removes only its own entry. No completed instantiation cache, member
image or semantic forcing is added. Full general reference-factory and native
alias-stack integration remain under `tsr-1yb.7.7.3` and `tsr-1yb.33.1`.

The two Rust controls pass; an actual compiling origin-substitution mutant
fails the clone control. The workspace passes 3,243 tests with 19 existing
ignores before test-module relocation, and the final library passes all 197
tests. Strict workspace Clippy reproduces the same seven inherited errors as
the corrected characterization baseline, with only template line offsets;
fmt passes. All 552,533 eligible type rows and 12,238 unfiltered diagnostic
cases are byte identical to the previously qualified `835ef559` baseline.
This is expected-driven preservation, not a fresh full-native parity proof.

The complete private owner draft now has 60 type mismatches, down from 69.
An intermediate pass exposed 13 borrow/move errors; cloning just the owner
handle and borrowing reference targets removed them. Binding-object owners,
class construction and global array/Promise comparisons retain actual owner
identity. Exact continuation patches, compiler diagnostics and failed setup
attempts are retained in the evidence. Its wider factory/mapper/natural alias
controls remain unexecuted. This ownership prerequisite claims no speed or
coverage gain; the equivalent complete-work TSR/native median <=0.50 remains
unverified.

### String mapping owner integration

Frozen main `189a2c1d` plus the five Rust overlays in
[the evidence](checker-string-mapping-owner.json) carries actual `SymbolRef`
owners through the existing string-mapping stores, recursion, constraint
rebuilding, substitution, inference and relations. Genuine bound alias entries
lift their input and use the same selected worker. There is no origin projection,
second worker, new cache or cross-Checker state.

Pinned native `5b1047d` `getStringMappingTypeForGenericType` keys its completed
image by the actual mapping symbol and target type. `getStringMappingType`
returns an existing image when its mapping symbol is the same; distinct clones
must remain distinct even with identical spelling and shared declarations.
Literal/union/template results still use structural reuse. The rejected
alternative of normalizing a clone to its origin merges generic image identity
and changes same-owner inference and relation behavior. Native direct-worker
controls confirm distinct owners/images, repeat reuse, shared completed literals,
same-owner inference, cross-owner refusal and nested substitution in both query
orders, before and after checking. All four rows repeat byte identically.

Both mutable maps remain Checker-local, keyed by validated owner and local
`TypeId`; they publish only completed generic image construction. This operation
has no active reservation. No captured mapper or alias frame is reused here;
the actual selected owner survives later target substitution. Generic image
construction still formats its target as before. Worker counts and allocation
cost are unmeasured (`tsr-1yb.11`); this correctness prerequisite is not a speed
win or authorization to memoize another computation.

The workspace passes 3,245 tests with zero failures and 19 existing ignores.
The isolated reader passes 212 library tests. An actual compiling origin mutant
fails one owner test while the genuine bound test passes. Unfiltered outputs
retain all 552,533 eligible type rows and 12,238 diagnostic rows byte for byte
against the source-qualified baseline. All 779 tracked build inputs match the
previous qualified baseline and main `189a2c1d`. Strict Clippy has the same seven
distinct inherited errors on baseline and candidate (seven versus twelve raw
messages because candidate targets repeat five errors); format checks pass.
These are expected-driven corpus gates, not a fresh complete native oracle.

The broader private continuation routes computed reference rebuilding through
the general selected factory and preserves selected alias/mapped/union and
variance state. Its compiler mismatches move through 60, 84, 65, 59 and 51 as
the owner boundary exposes further callers. Exact five/16-file continuation
deltas replay against the preceding artifact and retain all 1,053 input hashes
per slice. The full draft is still uncompiled; full factory/mapper, natural alias,
member publication and private variance controls remain unexecuted. The direct
native string worker controls do not stand in for those gates. All six goal
tickets remain unfinished; equivalent complete-work wall ratio <=0.50 remains
unverified.

### Selected class constructor and base owner continuation

After production delivery `2ae37af8`,
[the private continuation](checker-class-base-owner-continuation.json) carries
actual selected class owners through the existing constructor slot, base list,
heritage reference cache and narrowing/nominal/property metadata consumers.
Pinned native `5b1047d` operations are `resolveAnonymousTypeMembers`,
`getDefaultConstructSignatures`, `getBaseTypes`, `resolveBaseTypesOfClass`,
`resolveBaseTypesOfInterface` and `hasBaseType`. A private copy shares syntax
declarations, not its source's active frame or completed class-owned link.

The constructor placeholder remains the existing `None`; base lists preserve
partial re-entry and resolved publication. Heritage keys retain location and
ordered written argument syntax as well as the actual target. Alias-frame
admission, default substitution and publication guards keep their policy.
Genuine bound entry points lift Program-owned symbols; no private owner is
projected to an origin. This is an owner migration, not certification of native
`__constructor`/structured member publication or a new reuse policy. Expensive
worker counts and wall/CPU/RSS effects remain unmeasured in `tsr-1yb.11`.

The exact nine-file patch replays against the prior 1,053-input full draft,
with 1,044 inputs unchanged. Qualified checks retain 48, 41 and 38 errors in
order; the final 38 are type mismatches at other owner boundaries. The initial
sccache launch failed before compilation. No runtime, corpus or fresh native
class/base controls ran for this uncompiled draft. The six goal tickets and
the equivalent complete-work median TSR/native <=0.50 target remain unfinished.

## Consumer boundaries

The [member-origin continuation](checker-member-origin-continuation.json) after
`276f3419` replaces the existing `AnonymousProperty.origin` field with an actual
selected handle. Genuine syntax writers lift their Program IDs once; callable
exports retain their actual private edge. Spread, ordering, widening, reverse
inference, key extraction, relation metadata, excess-property reporting and
display readers use that record's flags, declarations and value declaration.
Existing property-type/display slots and publication/forcing policy remain in
place; no second origin image, member cache or origin-ID projection is added.
The property lookup result producer still returns raw IDs and is an explicit
remaining compiler boundary, rather than a reason to drop a private origin.

Pinned native `5b1047d` `compareSymbolsWorker` (`utilities.go:366`) compares the
selected first declaration and name before `ast.GetSymbolId` (`ast/utilities.go:34`).
The new identity slot belongs to each actual record: zero is uncomputed, a
winning atomic publication is stable, bound records share it across checkers,
and every private copy starts with a fresh slot. A global counter allocates IDs;
there is no global symbol cache. This replaces an intermediate, incorrect
checker-local sorting slot. IDs are never Program indexes, semantic cache keys
or substitutes for validated `SymbolRef` ownership. Selected sorting allocates
an ID only at a declaration/name tie. Other native identity consumers and the
historical raw comparator remain outside this bounded qualification.

Twenty-one actual binder tests pass, including concurrent publication and store
relocation. Three extracted store/comparator tests pass; an actual compiling
origin-substitution mutant fails the private-copy ordering test, then the
restored reader passes again. Two fresh pinned-native tests exercise comparator
identity/name/nil ordering and concurrent publication. The reader excludes full
Checker construction, member execution and TypeStore semantics. Full ordinary
and work-trace checks still fail with 84 type mismatches. Complete native member
behavior, prior RIGHT retention, performed work, expensive-worker counts and
whole-process wall/CPU/RSS remain unqualified in `tsr-1yb.4.2.1`,
`tsr-1yb.33.1`, `tsr-1yb.11` and `tsr-1yb.16.3.10`. Conditional binding keys
and alias consumers remain in `tsr-1yb.7.7.3`; `tsr-1yb.1` still owns the
equivalent complete-work median TSR/tsgo <=0.50 requirement.

The [alias-consumer private continuation](checker-alias-consumer-continuation.json)
after `c236a387` extends the existing declared dispatcher to selected symbols.
Native `5b1047d` `tryGetDeclaredTypeOfSymbol` (23678),
`getDeclaredTypeOfTypeParameter` (23829) and `getDeclaredTypeOfAlias` (24094)
provide its operation boundary. Type-parameter mints record the actual selected
symbol; alias dispatch uses the one resolver and retains the source alias's
completed declared link. Class/interface, type alias, enum and enum-member
branches remain in the same dispatcher. This is private implementation progress;
its current publication policies are not certified native completion.

The existing type-parameter back-edge, homomorphic variable and rendering scope
retain selected handles. Constraint/default/const/inferred-declaration reads use
that symbol's own metadata; fresh inference parameters retain its identity,
matching native `cloneTypeParameter` and `getRecursionIdentity`. Syntax-owned
shadow candidates and qualified namespace mints lift genuine Program IDs before
comparison/keying. Generic-reference/default/body work stays in the existing
worker with the original written site/alias policy. No new semantic cache,
mapper, producer or origin projection is introduced.

Callable export preparation reads actual selected export edges and declarations,
then uses the existing selected type getter and readonly/name workers. Its
`AnonymousProperty.origin` write exposes the remaining raw-ID member-image
boundary. The existing field and all spread/order/inference/reporting/display
consumers require one coordinated migration; dropping private origins or storing
a second parallel field would preserve the wrong owner model. Conditional
binding maps remain syntax-ID keyed and cannot accept a selected parameter
without their own boundary migration. These limits are recorded in
`tsr-1yb.4.2.1` and `tsr-1yb.7.7.3`; actual expensive-worker reuse remains
unqualified under `tsr-1yb.33.1`.

Exact 18-file replay verifies 1,053 inputs/1,035 unchanged. Source-stable
compiler counts are 116→93→89→84, with final ordinary/work-trace both84
E0308. Owned formatting passes; rustfmt's unrelated test-child change is restored
from the exact predecessor. Existing const/default/constraint/member tests were
inspected, but the private checker cannot execute them yet. No current native
matrix, runtime/full-corpus/no-prior-RIGHT-loss/performed-work or whole-CLI
wall/CPU/RSS qualification ran. All six goal tickets and the equivalent
complete-work TSR/native <=0.50 target remain unfinished.

The [alias-resolver private continuation](checker-alias-owner-continuation.json)
after `c92a8cb0` ports the existing alias links to selected owner and target
identities. Native `5b1047d` `resolveAlias`/`resolveIndirectionAlias`/`tryResolveAlias`
provide the operation boundary: uncomputed differs from completed unknown,
AliasTarget frames live in the shared resolution stack, publication precedes
pop, failed push does not publish, and failed pop reports at the actual alias
declaration before replacing the target with completed unknown. Type-only
declaration NodeIds stay in that same checker-local link; a source marker wins
over the target marker. No process/global state, origin projection, fabricated
ID, second target cache or second cycle producer is introduced.

The shared cycle search checks publication before identity and respects
`resolutionStart`; a positive read-only probe records the existing stack
observation without failing frames. Declaration checking and name suggestions
use that producer/probe, replacing their bounded walks. Actual syntax-owned
target APIs still supply Program IDs, explicitly lifted. Private member-image
producers and raw declaration/entity/export/naming/grammar/JSX/flow/reuse
consumers are remaining migration boundaries, not permission to demote targets.
The existing module-clone payload and alias value cache/frame retain selected
owners. Existing non-alias publication predicates, missing-module/error recovery
and type-only worker ordering remain unqualified against native behavior.

Exact nine-file replay verifies 1,053 inputs/1,044 unchanged. Widening alias
results exposes 100 type mismatches; selected suggestion/visibility/truthy
consumers reduce this to 93 in final ordinary and work-trace checks. Ten actual
generic stack tests pass and two mutants each fail one test, explicitly excluding
Checker-specific code. No alias runtime, native controls, full corpus, checked
scope, expensive-worker counts or wall/CPU/RSS qualification ran. All six tickets
and the equivalent complete-work TSR/native <=0.50 target remain unfinished.

The [value-worker private continuation](checker-value-owner-continuation.json)
after `640120e4` carries selected owners into the existing accessor,
function/class/enum/module and variable/property workers. Their type memo,
resolution frames and anonymous payloads retain the actual selected symbol.
The completed callable metadata and initializer/written-return comparisons also
retain that identity; a private copy cannot qualify by its original declaration.
Pinned native `5b1047d` supplies the operation boundaries, but this owner
migration preserves the existing Rust inference and publication policies.
It does not certify native accessor failed-pop recovery or the variable
worker's contextual-parameter publication exception.

Assignment-declaration classification keeps the existing `this_expando_kinds`
cache keyed by selected owner. Constructor-flow and base-property readers use
selected declarations and parent; the latter consumes the existing selected
base worker. Property and element-access callers explicitly lift the genuine
Program property they currently receive. Shared NodeId syntax still selects
Program symbols; it does not justify demoting a private owner. One existing
`trace_symbol_work` observes selected declaration files without asking for
types, and all three existing callers use it. No duplicate producer/cache was
introduced. Private declaration copying and worker counts still belong to
`tsr-1yb.11`.

Exact ten-file replay verifies 1,053 inputs with 1,043 unchanged. Actual checks
move 27→26→25 E0308; final ordinary and work-trace builds report the same 25.
The signature-result adapter is repaired, while assignment declaration
migration resolves the downstream variable branch. Naming/alias, callable
exports/property-origin images and member consumers still need owner migration.
No private runtime/native/corpus/performance test executed; no canonical Rust
changed. All six ticket gates remain unfinished.

The [index/dispatcher private continuation](checker-index-owner-continuation.json)
after `472c249f` carries selected owners in the existing index memo, visited
path, declaration scan, sibling reads and late-name cache/active marker keys.
Pinned native `5b1047d` `getIndexInfosOfSymbol` and `getIndexInfosOfIndexSymbol`
read actual member identities; shared declaration/heritage syntax still supplies
Program symbols, explicitly lifted at that boundary. Receiver substitution,
memo frame admission, publication marks and unsupported/error refusals remain
in their existing workers. This does not certify native `MembersResolved` or
natural late-member reset/publication. No duplicate cache or producer was added.

The existing `getTypeOfSymbol` flags dispatcher now takes a selected owner;
the raw public entry lifts a genuine Program symbol. Enum-member value links
and export-marker links retain selected identity, and exports re-enter that
same dispatcher. The accessor, variable/property, function/class/enum/module
and alias workers still need owner migration. Their four compiler mismatches
replace the four resolved index mismatches: the final error count remains 26.
Step12 also exposed one import and three assumed nonexistent getter errors;
step13 repairs these; step14 copies only the selected handle rather than the
full type payload and confirms the same 26 errors. All three actual compiler
receipts are retained. Exact five-file
replay verifies 1,053 inputs with 1,048 unchanged. Existing active-marker tests
only have key plumbing changed; no private runtime or native/corpus/performance
control executed. Bound query tracing stays at its public boundary; private
worker/copy attribution remains `tsr-1yb.11`. All six tickets remain unfinished.

The [member/enum private continuation](checker-member-enum-owner-continuation.json)
after `26da4dd0` migrates the existing declaration/flag metadata readers and
completeness paths to actual selected owners. Optionality, spreadability/privacy,
rest and tuple-relation iteration retain the selected member; bound wrappers
lift genuine Program symbols into one worker. Visited paths retain private
identity, while shared heritage syntax still selects actual Program bases.

Pinned native `5b1047d` `getDeclaredTypeOfEnum`, `getDeclaredTypeOfEnumMember`
and `getUnionTypeEx` keep enum-owned literal and declared-type links distinct.
The draft carries `SymbolRef` in enum literal payloads, value interning,
computed/fresh/regular owner links and the existing union worker. Declaration
syntax selects member symbols as native `getSymbolOfDeclaration` does; a private
enum copy does not invent a second syntax/member producer. Parent forcing and
fallback publication use the selected links. Completed printing and source reuse
compare actual handles, never a private copy's origin. Existing name/refusal,
recursion, reduction and singleton policies remain; native late parent and
full enum/member publication are still unqualified.

Exact 13-file replay verifies 1,053 inputs, with 1,040 unchanged. Qualified
compiler errors move 38→34→37→26, including five exposed iterator/borrow/move
errors that were repaired. The final 26 are owner mismatches. No private runtime,
fresh native enum/metadata, full factory/mapper/natural alias, corpus or CLI
performance control ran. Actual work/copy attribution remains `tsr-1yb.11`;
all six tickets and the equivalent complete-work <=0.50 target remain unfinished.

| Consumer | Required result and work after static selection |
|---|---|
| Identifier expressions (`expressions.rs`) | Value symbol, including a local import alias when native keeps it. Continue class-field initialization checks, value typing, assignment/write handling and flow narrowing. Flow results cannot enter the static symbol memo. |
| Type references (`declared.rs`) | Fully resolved type target; then apply current `alias_evaluation_bindings`, type arguments, arity/default checks and instantiation. Preserve written-reference and alias/origin presentation channels separately. |
| Qualified entity names (`declared.rs::resolve_entity_name`) | Namespace/export traversal with native alias/meaning rules. Raw `meaning | Alias` acceptance is not complete target resolution. Export-equals retry, CommonJS redirects and alias-chain meaning checks remain unported branches. |
| Alias targets (`symbols.rs::resolve_alias`) | Current main memoizes a bound target or `None` with a per-alias circular flag. This is not yet native AliasTarget stack/publication; `None` also includes unsupported work. Flattened targets and immediate declaration targets require distinct native entry points. |
| Heritage/member resolution (`members.rs`) | Resolved base/owner identity, followed by concrete arguments and receiver/member completion. A scope result is not a completed member image. |
| Flow (`flow.rs`) | Static value selection followed by the current flow node and narrowing context. Do not cache the narrowed TypeId in a static symbol entry. |
| Export target selection (`symbols.rs`) | Keep the local written symbol and exported/merged target roles distinct. Re-export and export-assignment callers are not automatically type-reference getter consumers. |

The measured [caller inventory](checker-symbol-resolution-callers.csv) includes
100 caller/meaning rows. It is a raw binder-query inventory, not a proof that
every row can share a native node completion. The native symbol links also have
writers for private identifiers, property accesses, late-bound declarations,
import-type paths, and symbol-at-location queries. The three characterized
getters do not authorize a generic memo for those writers or all entity-name
queries. Extend the consumer audit before broadening the entry surface.

## Controls and limits

[Control results](checker-symbol-completion-contract-controls.json) retain all
25 native getter observations, actual worker-branch counts, report-once error
counts, pointer assertions and forward/reverse type-query sequences. The final
runner asserts the native type strings; it does not only print them. Two
Checkers on the same Program reuse the ordinary bound declaration but have
different unknown sentinels and unresolved-symbol pointers. Same-spelled type
and value references, lexical shadowing, missing syntax, local import aliases,
fully resolved imported type targets and const symbol queries are covered.

The runner is a standalone assertion executable built with existing native
dependencies. Offline `go test` could not execute because the separate
`gotest.tools/v3` test dependency was uncached. No dependency was installed.
Timing/resource observations locate controls only; they do not support a
whole-project performance ratio. The first two saved samples precede the final
reverse-query/assertion additions; source hashes in the artifact refer to the
final sample. All sample binary identities remain separate.

Reproduction: export the exact pinned native tree, apply
[the existing getter probe](checker-symbol-completion-probe.patch), then apply
[the contract delta](checker-symbol-completion-contract-probe.patch). Build
`./cmd/symbol-completion-contract` with the native Go version and
`-buildvcs=false`; run with `TSR_NATIVE_SYMBOL_PROBE=1`. The delta adds two
scratch-only files and does not modify the getter semantics. Patch application
and resulting source hashes are verified independently.

[Storage controls](checker-symbol-storage-contract-controls.json) add nine
direct native cases with **88 passing assertions** for private clones across
Checkers, zero CheckFlags, independent member/export map mutation, declaration
append, bound-origin redirects, interface/namespace merging, raw versus queried
parent identity, unidirectional source redirects and same-symbol no-op merges.
The normal const type-query control leaves its symbol link uncomputed, reuses
the completed type, prints `{ readonly value: 1; }` and adds no name error.
They call the original native methods on bound symbols; the standalone runner
uses existing cached dependencies. Apply the base getter probe (disabled for
this run), then [the storage delta](checker-symbol-storage-contract-probe.patch),
build `./cmd/symbol-storage-contract` with `-buildvcs=false`, and run it without
`TSR_NATIVE_SYMBOL_PROBE`. The saved result includes complete output and binary/
source identities. The earlier eight-case sample retains its own historical
source hashes; final hashes describe the nine-case runner. These controls
deliberately invoke the merge worker; they do not reproduce full source-level
module-augmentation preparation.

The seven Rust ownership-layout controls pass in
`cargo test --release -p tsr-checker --test symbol_domain_contract`. They use a
test-only Program stamp wrapper over the real bound store and private records;
the production Program and Checker have not acquired these stamps or accessors.

The Rust unresolved-reference fixture passes repeated/reordered queries for
full qualified paths and different type arguments. The retained native
expectation for `type Identity<T> = T` is ignored under **`tsr-6.57`**: TSR
prints `Identity<string>`/`Identity<number>` where native type queries return
`string`/`number`. `get_instantiated_type_reference` has position-sensitive
expansion/presentation and a named-reference fallback; the mismatch does not
prove a substitution-frame leak. Do not change the expected native strings to
make the test pass or infer that dynamic instantiation is safe to memoize.

Late augmentation and merge-order behavior is unverified here and already has
the concrete fidelity blocker `tsr-6.49`. Its retained ownership control stays
ignored, not certified as passing. The representation follow-up must integrate
that contract before enabling shared preparation or production worker reuse.

Production lookup reuse remains **`tsr-1yb.7.7`**. Refresh current-source
expensive-worker counts, implement only a supported completion boundary, and
require complete diagnostics/performed scope, no previously RIGHT corpus losses
and independently confirmed fresh-process full-project benefit. The release
target stays verified TSR/native median wall ratio **at most 0.50**; these
characterization controls do not establish it.


## Production foundation verification

The [production evidence](checker-symbol-production.md) records the first
integrated ownership/storage prerequisite on b0475d3b. Fifteen domain controls
exercise the real accessors, while a real Checker control verifies private
unknown ownership across two Checkers sharing one Binder. The final AST-segment
unresolved-reference implementation preserves all 474,251 type-result rows and
10,570 diagnostic cases byte-for-byte. The unchanged app check retains the same
1,341 checked identities and all 120 diagnostics.

Two five-pair normal-CLI overhead comparisons have opposite signs; this is no
confirmed performance gain or regression and establishes no native speed ratio.
The read-only storage snapshot records retained capacities without publishing
completion or counting allocation/clone events. Further production cost
attribution and absent/empty bound-table fidelity remain in storage task .7.7.1;
consumer migration, alias completion and node reuse stay separately gated.


The delivery rebase onto `9a44a194` repeats the full preservation gate against
its 459,549 RIGHT baseline, passes 140 focused tests with one retained ignored
control and preserves every full diagnostic case. It also covers a native raw
parent with an empty name. Separate rebase identities are recorded in the
production evidence; initial timing/storage controls remain attributed to b047.

The [property-owner continuation](checker-property-owner-continuation.json) after
`39cdcdd9` widens the existing property and export-star result channels,
composite constituents and static/instance name walks to selected handles.
The existing structured-name table and path guard use actual selected owners;
publication marks, cycle/unsettled state and frame policy are retained, with no
new cache. Genuine bound producer outputs are lifted at their boundaries.
Final ordinary/work-trace both retain 80 compiler errors. One existing actual
store-copy/member/export control passes in extraction; it does not execute the
property producer or certify full member/alias/native/corpus/speed behavior.
Remaining property diagnostics, entity/module/naming and conditional-frame
channels still block the six-ticket goal and canonical runtime integration.

The [binding-owner continuation](checker-binding-owner-continuation.json) after
`bda345a6` carries actual selected handles through existing alias frames,
captured conditionals and default/constraint/type-literal keys. Genuine AST
parameter inputs lift at their binder boundary; private mapper symbols retain
their owner. Flattening preserves inner shadowing, and the existing selected
symbol comparator normalizes key vectors without an origin projection. Existing
scope/publication gates remain, with no new memo or broadened completed-state
claim. Selected delete/callee/heritage/index diagnostics read actual metadata.
Three bounded extracted key controls pass, catch a compiling origin-projection
mutation, and pass after restoration. The bounded fixture is explicitly distinct
from full Checker construction/evaluation. Ordinary/work-trace71 errors and the
full lib-test no-run refusal keep canonical integration and all six goal tickets
unfinished; native/corpus/performed-work and equivalent-work speed gates remain.

The [alias-result continuation](checker-alias-result-continuation.json) after
`822edfa4` carries selected symbols through the existing declaration/import/export
and entity-name workers. Real binder results lift at the syntax/Program boundary;
property and star results keep their private identity. Existing qualified generic
reference keys now use that owner with their written spelling and ordered type
arguments. Shallow missing-namespace/export diagnostics, spelling tables and
emit-helper signatures read selected metadata. There is no extra alias/export
memo; existing AliasTarget links own cycle/unknown completion, so the entity
reader no longer imposes a 64-hop limit. The separate bound external-module
lookup keeps its genuine Program contract, while semantic selected-module reads
can follow a private export-equals edge.

Two bounded extracted actual own-export tests preserve distinct private table
edges, original/sibling independence, the unchanged bound entry and module guard.
Separate compiling origin and guard mutations each fail one; restored two pass.
Actual parser/binder/store and export/star reader bodies are used, but a bounded
Checker constructor and opaque TypeId replace full Checker setup. External module
resolution panics and is never entered; no star graph, alias/default/supplemental
or type-only publication qualification follows. Setter/constructor setup errors
are retained separately from semantic mutation failures. Final57 ordinary and
work-trace errors/full-lib-test113 still refuse canonical runtime integration.
Semantic naming and conformance query consumers remain unfinished, as do the
native augmentation/combined-symbol gaps and all six goal acceptance gates.


The [selected naming continuation](checker-semantic-naming-continuation.json)
after `b96ed671` migrates the existing best-name, immediate own-name alias, clone,
module alias and export-equals naming consumers. Bound scope lookups lift at
their Program boundary; targets, selected parents, export markers and export
values keep actual `SymbolRef` identity. The native selected comparator orders
actual candidates. Short-lived views release before recursive forcing. Existing
AliasTarget links remain the sole publication owner; candidate tables are local
to the query and introduce no memo. Module property display follows selected
member edges and existing semantic type forcing, stack and rendering admission.

Native `getSymbolTableAliases` caches alias-only globals/raw/resolved export
tables by table identity, filters uncached locals and skips members tables.
TSR still copies full naming tables; `tsr-1yb.11.5` owns actual scan/copy costs,
table lifetime and mutation/publication controls before any cache implementation.
The selected property comparator's traversal cost is likewise unmeasured. This
ownership continuation establishes no allocation or speed benefit.

A bounded extracted actual direct naming control keeps two same-named private
copies inaccessible through the original class's lexical name. It passes,
detects a compiling origin-substitution mutation, and passes after exact
restoration. Actual parser/binder/store/worker are used, but the constructor is
bounded and alias/qualification paths panic if entered. It does not certify
alias/export traversal, module sorting/serialization or full Checker evaluation.
Ordinary/work-trace34 errors and full-lib-test90 still refuse runtime integration.
All six goals and full native/corpus/performed-work/median ratio<=0.50 gates remain
unfinished; prior augmentation/combined-symbol and naming admission gaps persist.
