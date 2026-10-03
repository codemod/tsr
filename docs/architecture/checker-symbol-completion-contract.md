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
| Unknown sentinel | Private Checker | Intrinsic error/any TypeIds distinguish type outcomes, but are not a native unknown-symbol handle. |
| Unresolved path and its parents | Private Checker synthetic symbols | `unresolved_type_reference` creates an error-like named TypeId and registers it in `unresolved_types`. This preserves tested printing/error propagation, not synthetic symbol identity. |
| Merged/cloned transient symbols | Checker preparation/resolution as required by native | The ordinary bound-pointer control does not establish this behavior. See `tsr-6.49` and `tsr-1yb.3.2`. |
| Expression/type completion | Private Checker | `node_types`, `symbol_types` and instantiation tables store TypeIds. They do not substitute for static symbol completion. |

Rust's opaque SymbolId contains a store index without a domain tag. SymbolStore
holds borrowed names, declaration ids, members, exports and parent SymbolIds;
its symbols lack native transient CheckFlags. Checker borrows Binder
immutably. Appending synthetic ids to Program storage or passing private ids to
`binder.symbols().get` would violate these boundaries.

The concrete follow-up is **`tsr-1yb.4.1.6.1`**: specify the smallest distinct
completed-symbol handle, checker-owned synthetic store and accessor surface.
An implementable design must represent uncomputed separately from completed
unknown; distinguish bound handles from private handles; retain full parent
paths and alias origin; and prevent private handles from reaching a different
Checker. It must also cover native merged/cloned targets rather than assuming
all nonunknown symbols are Program ids. Storage layout is deliberately not
implemented before this consumer/domain obligation is settled.

## Consumer boundaries

| Consumer | Required result and work after static selection |
|---|---|
| Identifier expressions (`expressions.rs`) | Value symbol, including a local import alias when native keeps it. Continue class-field initialization checks, value typing, assignment/write handling and flow narrowing. Flow results cannot enter the static symbol memo. |
| Type references (`declared.rs`) | Fully resolved type target; then apply current `alias_evaluation_bindings`, type arguments, arity/default checks and instantiation. Preserve written-reference and alias/origin presentation channels separately. |
| Qualified entity names (`declared.rs::resolve_entity_name`) | Namespace/export traversal with native alias/meaning rules. Raw `meaning | Alias` acceptance is not complete target resolution. Export-equals retry, CommonJS redirects and alias-chain meaning checks remain unported branches. |
| Alias targets (`symbols.rs::resolve_alias`) | Resolve declaration/import/export targets. This resolver is currently intentionally unmemoized; do not substitute the alias getter's nonpublishing fallback for native aliasTarget completion. |
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
