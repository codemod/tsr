# Site-aware native reference display

`tsr-1yb.16.3.2.1` establishes the oracle for the written-reference contract.
It changes no production checker behavior and claims no speed gain. The
discarded-spelling candidate remains gated by `tsr-1yb.16.3.2.2` and its parent.

## Source and protocol

Rust library source is frozen at
`ecacd9d04c48c9d0192510289be664b189634c05`; native is pinned at
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. The normal source delta through
`4b1426e3` is the previously delivered allocation report and controls, with no
runtime changes. Both helpers are archive-only commands. The
[receipt](reference-spelling.json) records their source/binary hashes, options,
fixture bytes, all selected baseline types, full native diagnostics and
declarations, input digests and terminal child identities.

Plain `Checker.TypeToString(t)` is **not** the canonical native `.types`
rendering protocol. For these selected nodes,
`internal/testutil/tsbaseline/type_symbol_baseline.go::writeTypeOrSymbol` uses:

```go
ctx, putCtx := printer.GetEmitContext()
defer putCtx()
ctx.Reset()
builder := checker.NewNodeBuilder(fileChecker, ctx)
flags := checker.TypeFormatFlagsNoTruncation |
    checker.TypeFormatFlagsAllowUniqueESSymbolType |
    checker.TypeFormatFlagsGenerateNamesForShadowedTypeParams
node := builder.TypeToTypeNode(t, selectedNode.Parent,
    nodebuilder.Flags(flags & checker.TypeFormatFlagsNodeBuilderFlagsMask) |
        nodebuilder.FlagsIgnoreErrors,
    nodebuilder.InternalFlagsAllowUnresolvedNames, nil)
```

The printer removes comments, writes against the selected source file and uses
an empty newline separator. Intrinsic `any` uses its intrinsic name. Selected
probes are **27 top-level variable/function declaration identifiers**; the
native walk's type-alias-name and class-heritage special cases do not apply.
This helper does not claim to replace the full native baseline walker.

The [native helper](reference-spelling-native.go) runs one protocol per process:
`baseline`, `plain`, `missing-enclosing`, or `missing-flags`. Each forward and
reverse order has a fresh Program and private checker, with single-threaded
checking. Types are queried before root checking and again afterward. It then
releases the checker before collecting complete program diagnostics and native
declaration emission. The deliberately broken protocols retain the baseline
options and inputs. Diagnostics, declarations and loaded-text hashes agree
across all four protocols and both orders.

The [Rust helper](reference-spelling-contract.rs) uses the configured conformance
producer, followed by public queries on fresh configured checkers in forward and
reverse order. It reports fixture unit bytes as UTF-8 hex. It does not perform a
full source check or declaration emit: those are separate public/compiler
controls, not implied by query agreement. No Rust declaration equivalence is
claimed here. Native declaration-only options are recorded separately from the
three shared query options (strict, skipLibCheck, esnext).

## Executed controls

The [driver](reference-spelling-controls.py) completed 55 fresh helper children
across 11 families: defaults, nested, dependent, recursive, mapped, conditional,
qualified, cross-file, unique symbol, deliberate error and long union. Every
native and Rust fixture unit has identical bytes. The final driver batch was
preceded by a separate complete 55-child batch; all native/Rust observables,
including measured instantiation deltas, agree between batches. Their raw
receipts remain in the local archive; the final compact receipt is tracked.

The protocol distinction is executable:

| Control | Canonical site-aware output | Broken/alternate output |
| --- | --- | --- |
| Bare parameter | `(x: Box) => Box<number>` | plain/no enclosing: `(x: Box<number>) => Box<number>` |
| Partial parameter | `(x: Pair<string>) => Pair<string, string>` | plain/no enclosing: `(x: Pair<string, string>) => Pair<string, string>` |
| Unique symbol | `unique symbol` | missing flags: `typeof probe_symbol` |
| Long union | All 64 return constituents | missing flags truncates the return union |

Computed variable/reference types expand defaults, while written function
parameter annotations can preserve their source spelling. Required written
reuse therefore cannot be tested against plain `TypeToString` output. The
initial plain-protocol captures are retained as locating evidence and are not
classified as failed native fidelity.

Native site-aware **printing actually instantiates** types in these controls:

| Family / selected probe | Forward before-check print delta | Reverse before-check print delta |
| --- | ---: | ---: |
| defaults / `probe_fnPartial` | 1 | 1 |
| dependent / `probe_fnChain` | 1 | 1 |
| mapped / `probe_fnMapped` | 0 | 2 |
| conditional / first queried probe | 1 (`probe_conditional`) | 4 (`probe_fnConditional`) |

The counter is native `TotalInstantiationCount`, read separately around querying
and rendering. It counts actual instantiations, **not every lazy effect**;
zero does not prove purity. Query order changes which operation performs the
work even when outputs match. Rust `type_to_string(&self)` formats an existing
Type, whereas contextual/native node building has different forcing boundaries.
Only an audited exact Rust branch may defer discarded formatting.

Nine families match the canonical native display. Two retain seven existing
differences, rather than changing expected output:

- `tsr-6.23.1`: a written `Outer<Select<number>>` function parameter is reduced
  by Rust; native preserves `Select<number>` there and reduces the return.
- `tsr-6.67`: `import Alias = Left.Shape` used as `Alias<string>` answers Rust
  `error`, while native resolves the generic alias.
- `tsr-6.66`: bare qualified generic receivers retain missing defaults in Rust.
  The qualified family also records alias-versus-written parameter spelling;
  its repair needs the existing written-node/alias contracts.

Native accepts all positive families and reports exactly one TS2322 for the
deliberate error. Full messages, ranges, chains and related information are
retained. These fixtures are bounded contract evidence, not whole-project
performed-work equivalence or the required median TSR/tsgo wall ratio <=0.50.

## Reproduction

Use an isolated checkout of the native pin. Copy `reference-spelling-native.go`
to `cmd/reference-spelling/main.go` and build that command. Copy
`reference-spelling-contract.rs` to the frozen Rust checkout's
`crates/tsr-conformance/examples/reference_spelling_contract.rs` and build only
that release example. Use existing dependencies offline; neither helper adds a
dependency or modifies the production checkout/vendor tree.

Each build receipt must contain `terminal: true`, `exit_code: 0`,
`helper_sha256`, `binary_sha256`, and the respective `native_sha` or `source`.
The tracked report contains the exact archived build commands and hashes.
Supply the binaries and their verified receipts:

```sh
python3 docs/architecture/reference-spelling-controls.py \
  --native /absolute/archive/native-reference-spelling \
  --rust /absolute/archive/release/examples/reference_spelling_contract \
  --native-build /absolute/archive/native-build.json \
  --rust-build /absolute/archive/rust-build.json \
  --directory /absolute/archive/new-oracle-run
python3 docs/architecture/test-reference-spelling-controls.py
```

The destination must be fresh. The driver verifies and persists each child
before interpreting it, exports `results.json` and `summary.json`, and rejects
partial processes, wrong binaries/sources, missing phases/probes, wrong orders,
changed diagnostics/declarations and ineffective broken-protocol controls.
Four reader tests include 20 negative variants. No candidate is kept from this
oracle; full conformance and ordinary whole-CLI measurements remain required.
