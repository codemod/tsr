# Accessible symbol chains include namespace exports

`Checker::best_name` is the site-aware naming slice of native
`getAccessibleSymbolChain` (`symbolaccessibility.go:373`). Native
`someSymbolTableInScope` checks each namespace declaration's locals and then
its exports (`symbolaccessibility.go:746-775`). The Rust walk previously checked
only locals and globals.

That omission matters for `export import Alias = M`: the alias is stored in the
namespace's exports table. When a type under `M` is serialized from that
namespace, native can emit `Alias.C`; the Rust fallback emitted `M.C`.

The port now adds each enclosing namespace's exports as a distinct scope table,
preserving native ordering and direct-hit-before-alias selection. It does not
widen alias resolution or module-object naming. Controls cover both a matching
alias in the immediate namespace and a non-matching inner alias followed by a
matching outer alias.

At base `8f8f4e1a`, the full checker-types scorepair reports no corpus
transitions. The change is retained because the native invariant is established
independently by focused controls; no printed-result special case is involved.
