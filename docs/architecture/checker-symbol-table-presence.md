# Native bound symbol-table presence

The bound symbol table port preserves native nil versus initialized-empty state
without widening the bound symbol record. `Symbol.members` and `Symbol.exports`
now use `SymbolTableField`, an explicit `Option<SymbolTable>` wrapper. Local and
global scope tables retain the existing `SymbolTable` alias.

Reads, iteration and missing mutable lookups preserve absence. `initialize`
matches native `GetSymbolTable`; insertion initializes the table, while clearing
or removing its final entry preserves presence. Binder merges propagate source
presence even with no entries. Bound-to-private clones copy presence and own
their tables independently, keeping shallow symbol edges and immutable sources.
Presence is not a completed-member predicate.

The native oracle is commit `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Its characterization passes 171 assertions across fifteen controls, including
nil/empty merges and source-preserving clones. The portable probe patch adds
only observation files and applies to the clean pinned tree. Original native
clone, merge, binder and AST implementations are checked by source hashes.
Raw internal symbol keys are recorded as hexadecimal: the native call key is
`fe63616c6c`; JSON's replacement-character rendering does not preserve its bytes.

The public field type changes from `SymbolTable` to `SymbolTableField`.
Borrowed generic lookup, indexing and reference iteration remain available;
callers needing the raw map use `as_ref` for reads and `initialize` for deliberate
publication. Workspace checks include all targets. There is no `Deref` to a
shared empty map, unsafe lifetime extension, new dependency or global semantic
cache. Retained bound size is 128 bytes and both raw map and field occupy 32
bytes on this arm64 build.

Two pre-existing bound-content reductions remain in
`tsr-1yb.7.7.1.1.1`: native classes publish a `Property|Prototype` export and
anonymous function/constructor types publish distinct signature members. The
port initializes those tables at their native stage but does not fabricate
their missing edges. The metadata characterization therefore does not certify
a complete native class/signature image. Parent storage and table-fidelity tasks
remain open; private consumer migration, alias publication and completed-node
reuse retain their existing gates.

Temporary profiling counts reference operations and table accesses only during
the observed compilation interval. Component loops are separate observations;
they do not predict whole-project savings. Inline record bytes and retained map
capacities exclude hash bucket, allocator and stamp overhead. Capacity growth
events are not allocator event counts. Whole-CLI timings use normal executables
after profiling source is restored, fresh compiler processes, explicit identical
libraries and disabled incremental/composite reuse. A baseline-TSR comparison
does not prove the pinned-native two-times target.

The initial validated source pair is current-main `ebb65c8a` and the owned
source hashes in [the controls](checker-symbol-table-presence-controls.json).
All 474,251 type assertions, including 459,598 RIGHT rows, and all 10,570
diagnostic cases are byte-identical. Workspace checks cover all targets;
282 focused release tests pass with two existing ignored tests (`tsr-qk9` and
`tsr-6.57`). The complete application checks the same 1,341 identities in order
and produces the same 120 complete diagnostics in enabled, repeated and
disabled probes. Temporary probe source is restored before timing.

The two five-pair comparisons observe candidate-minus-baseline median wall
changes of 11.684 ms and 24.744 ms.
These are small observed overheads, not a speed win or evidence of the native
0.50 target. CPU/RSS ranges and all binary/source identities remain in the controls.

The app has 1,082,855 bound symbols with 2,097,152 reserved slots (256 MiB of
inline reserved records). Only 145,798 member fields and 26,324 export fields
are present. Capacities remain unchanged by this port. This is the measured
input to compact-table experiment `tsr-1yb.7.7.1.1.3`; reserved bytes do not
prove committed RSS, and pointer indirection and present-table allocations
must be measured before retaining that candidate. No bound-handle or private
clone calls occur on this workload yet; future consumer costs need fresh
measurement rather than extrapolating from the 34,572 private stamp clones.

Replay the native patch in the exact pinned tree with Go 1.26 using the
existing dependency cache and `go build -buildvcs=false ./cmd/symbol-table-presence`.
The Rust controls live in binder unit/bind/program tests and checker
`symbol_domain_contract`. Profiling patches are scratch-only: apply to the
stated baseline/candidate snapshot, build release `tsr`, set `TSR_TABLE_PROFILE=1`,
then restore source. Use normal frozen binaries for paired timing.

## Delivery rebase

The delivery rebase is onto concurrent main `3255e43e`; code commit `bbb11d42`
retains the exact seven measured Rust source files. Fresh baseline/candidate
release runs preserve all 474,251 assertions including 459,614 RIGHT rows and
all 10,570 complete diagnostic cases byte-for-byte. The workspace all-target
check, strict affected Clippy and 306 focused release tests pass, with the same
two pre-existing ignored gaps. The original test invocation named a conformance
target in the checker package and ran no tests; the corrected checker command
and separate concurrent semantic tests pass.

Fresh enabled/disabled/repeated probes check the same 1,341 file identities in
order and preserve all 119 complete app diagnostics, including the concurrent
main improvements. Normal frozen executable medians observe +8.224 ms and
+11.528 ms candidate overhead in two independent five-pair rounds. These remain
small overhead observations rather than a speed win. Earlier ebb65 timings and
120-diagnostic observations retain their own source attribution.

Native raw signature names and current valid `__` spellings are not equivalent
keys. `tsr-1yb.7.7.1.1.1.1` owns disjoint internal/source-name controls before
signature edges or name projections are published. Compact-table candidate
`tsr-1yb.7.7.1.1.3` is an independent allocation experiment under `tsr-1yb.16`;
it does not block completion of the actual native table semantics. Storage and
table-fidelity tasks remain open for their real content/key/production gates.
The comparable pinned-native median target remains 0.50 and is unverified.

## Latest main validation

The final isolated code is rebased onto `31b00461`. Its complete type output
is `TOTAL 474251  right 459627  gap 2182  wrong 12442` and is byte-identical to that baseline. All 10,570 complete
diagnostic cases also match byte-for-byte. The workspace all-target check,
strict affected Clippy and 388 focused release tests pass; the same two
existing tests remain ignored. These checks include the concurrent relater,
conditional-target and signature-position changes.

Fresh normal/disabled/enabled/repeated app runs preserve the same
1,341 actually checked identities in order and all
119 complete diagnostics. Two independent five-pair
normal-executable comparisons observe candidate-minus-baseline median wall
changes of -18.129 ms and -73.375 ms. These costs are attributed
to this exact source pair; earlier source-pair measurements remain historical.
The observed direction varies across the saved source pairs, and these rounds
have substantial spread. No confirmed whole-project speed gain is attributed
to this semantic prerequisite. The pinned-native 0.50 target remains unverified.
Temporary profiling source is restored and verified before normal timing and delivery.

## Consumer and mutation audit

`SymbolStore::create` is the bound-record constructor and starts both fields
absent. Binder `declare_into_with_excludes` inserts or replaces member/export
edges; JavaScript/CommonJS declaration paths insert through the same field API.
`this_property_table` deliberately initializes before returning a mutable raw
table. `bind_container` publishes class exports and anonymous signature members
at the native stage. `merge_symbol` propagates source presence before merging
edges and retains existing target presence. Reads and missing mutations do not
initialize; removing the last entry and clearing preserve publication.

`BindResult` scope/name/export lookup and checker member, alias, accessibility,
signature and diagnostic consumers read the bound tables. Conformance binder
and type producers also only read them. Other crate matches are AST members,
package exports or source text, rather than bound-table mutations. Checker
private writes remain in `CheckerSymbols`, which validates ownership and clones
bound table presence independently. Repository searches found no second bound
constructor or production caller of table-presence/private-clone completion
outside that access module. This inventory is for the stated source snapshot.

The changed exported fields are `Symbol.members` and `Symbol.exports` plus the
new `SymbolTableField` export. Existing locals/global scope table APIs remain
`SymbolTable`. Generic borrowed lookup, indexing, reference iteration and raw
immutable borrowing retain their ordinary edge behavior; mutable raw access
requires explicit initialization. Full workspace all-target compilation covers
the existing consumers. The separate native synthetic-content/internal-key
tasks remain required before declaring completed member images reusable.

Prepared main snapshot `8ad07e6a` had the same tracked source tree as the isolated
candidate. Its separately rebuilt normal release CLI independently preserves
all 119 complete app diagnostics. Its executable byte hash differs from the
frozen isolated binary, so saved timings remain attributed to that exact
frozen artifact; they are not measurements of the rebuilt main executable.
Both identities and source hashes are recorded in the controls.

## Concurrent mapped-property rebase

Prepared rebase snapshot `b9ebab63` incorporated `c1ffade9`. All seven measured
Rust source files are unchanged. Incoming consumers use ordinary table reads;
no production presence/private-clone completion caller is added. Current checker
focus tests and mapped-property tests pass, together with a workspace all-target
check, strict affected Clippy and formatting in the isolated cache. The cold
main metadata check was manually interrupted and is recorded separately.

Fresh current-baseline and rebuilt main-candidate release binaries preserve all
118 complete app diagnostics, retaining the upstream improvement from 119. The
main release binary matches the frozen current candidate hash. Full-corpus and
paired-cost observations above remain attributed to their measured source pairs;
this rebase adds compatibility and complete-app checks, not a new full-corpus
or performance measurement. Native content/key gates and the 0.50 target remain
open.

The next clean rebase includes `a7bbb454` intersection/conditional reduction.
The same seven measured Rust files remain unchanged; current conditional,
relation, symbol-domain, merge and naming tests pass with workspace all-target,
strict affected Clippy and format checks in the cached worktree. Fresh baseline
and rebuilt main release binaries again preserve all 118 complete app diagnostics.
Counts and fingerprints are tracked; raw private-app diagnostics remain local.
These are source-attributed compatibility checks; earlier full-corpus and timing
observations retain their original source attribution.
