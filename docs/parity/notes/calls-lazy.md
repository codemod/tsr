# Lazy signature parameter demand — tsr-2zk.9.7

## Current reproduction

Investigated on parent `5dd3bad84d12991e1ba169d2d5687321e1989740`, against
`vendor/typescript-go` commit `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The native CLI was built directly with Go 1.26.8; Cargo used the installed
pinned Rust 1.96.0 toolchain. No offline bootstrap or tracked setup changes
were necessary.

Run both CLIs with `--ignoreConfig --strict --noEmit --pretty false` on:

```typescript
function getValue(argument: typeof value): number { return 1; }
const value = getValue(1);
function text(argument: typeof message): string { return 'ok'; }
const message = text('hello');
```

TSR emits TS7022 at `(2,7)` and `(4,7)`:

```text
'value' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.
'message' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.
```

Pinned native emits no diagnostics. This is a current annotated-parameter
reproduction, not evidence inferred from historical case names.

A distinct, genuinely circular control must retain native diagnostics:

```typescript
function circular(argument: typeof result) { return argument; }
const result = circular(1);
function getNumber(argument: number): number { return argument; }
const invalid = getNumber('text');
```

Pinned native emits, in order:

- `(1,10)` TS7023: `'circular' implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.`
- `(1,19)` TS2502: `'argument' is referenced directly or indirectly in its own type annotation.`
- `(2,7)` TS7022: `'result' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.`
- `(4,27)` TS2345: `Argument of type 'string' is not assignable to parameter of type 'number'.`

Current TSR emits only the latter two. Removing all circularity diagnostics
would be wrong; the return/parameter resolution ordering also matters.

## Native operation and existing boundary

`Checker.getSignatureFromDeclaration` in `internal/checker/checker.go` stores
parameter symbols and publishes `signatureLinks.resolvedSignature` without
reading their types. The signature initially has unresolved return and
predicate slots. `Checker.getTypeOfParameter` reads `getTypeOfSymbol`, then
adds declaration optionality. Fixed-position consumers enter through
`Checker.tryGetTypeAtPosition` in `internal/checker/relater.go`; rest-position
consumers read the rest symbol type separately. The node builder's
`symbolToParameterDeclaration` / `serializeTypeForDeclaration` performs
written-node reuse at serialization time, not at signature construction.

TSR already has `Parameter`'s private `Slot::{Resolved, Symbol}` in
`signatures.rs`. However, `parameter_of` uses `Symbol` only for an
unannotated parameter whose own symbol Type resolution frame is already
active and whose type is not published. All other identifier parameters
resolve annotations or symbol types eagerly. The annotated reproduction
above therefore still closes a cycle during construction that native does
not construct.

The existing `parameter_type` accessor can perform symbol-backed demand,
using the existing `symbol_types` cache and `resolutions` stack. It is not
necessary to invent a second semantic cache. Fully changing construction
also requires preserving written-annotation reuse and contextual/mapped
parameter images; merely broadening the unannotated special case does not
port this root cause.

### Ownership, publication, context, work

- **Key/value:** parameter identity is `SymbolId` in this checker's binder;
  resolved values are `TypeId` in its type store. An instantiated or
  contextually assigned parameter instead retains its resolved image.
- **Owner/lifetime/options:** the private Checker owns `symbol_types` and the
  Type-resolution stack for its checking lifetime. Optionality depends on
  `strict_null_checks` and the declaration's initializer/question token.
  Types computed under alias/mapped/contextual bindings must not be reused
  as an unrelated original declaration's type.
- **Publication:** an absent symbol entry is uncomputed; a Type frame on
  `resolutions` is active, not completed. The existing symbol worker
  publishes the result in `symbol_types` after computation. A symbol slot
  carries identity, not a provisional resolved answer. TSR's unsupported
  `error` type is not proof of native's completed error type. The current
  `parameter_of` error-annotation fallback must be addressed at lazy demand,
  not silently retained as an eager producer or replaced by suppression.
- **Consumer context:** `this`, rest versus fixed position, optionality,
  written aliases and annotation origin, original versus instantiated
  signature, and alias-evaluation mapper context remain distinct. Native
  semantic optionality and printer-written annotation are not the same
  slot representation.
- **Expensive work:** `get_type_of_symbol` / its variable-parameter-property
  worker and `get_type_from_type_node` are the current computation boundary.
  `symbol_types` already owns completed reuse; no extra cache was added.
  Worker executions, active repeats, completed hits, copies and whole-project
  benefit were not instrumented or measured in this investigation.
  **Integrator Beads follow-up request:** track those demand/publication
  counts and mapper-context equivalence under tsr-2zk.9.7 before extending
  reuse. This Box has no authorized Beads mutation scope.

## Required serialized cross-owner changes

No semantic implementation was applied: the faithful cutover crosses
explicitly forbidden whole-file boundaries. The integrator must coordinate:

1. **`crates/tsr-checker/src/node_reuse.rs`: `WrittenAnnotation`,
   `Checker::reuse_annotation`, its equivalence/serialization readers.**
   A written annotation currently stores an already-resolved `TypeId`;
   signature construction cannot populate that identity without resolving
   the parameter. Provide a real demand-time annotation path that preserves
   the original node and mapping context, and applies the existing
   `pseudoTypeEquivalentToType` rules only when serialization demands it.
   Do not store `error` as a fake unresolved equivalence identity.
2. **`crates/tsr-checker/src/checker.rs`:
   `Checker::signature_member_text_at`.** Replace its direct
   `parameter.written_text` read with the demand-time annotation accessor
   agreed with the calls owner, after semantic parameter demand. Preserve
   current reference-site and alias rendering. This file is not owned here.
3. **`crates/tsr-checker/src/objects.rs`: `signature_member_text`.** Replace
   the direct `parameter.written_text` read with the corresponding site-free
   demand-time annotation accessor. Preserve declaration syntax and
   predicates. This file is not owned here.
4. **Inference preparation contract:**
   `inference.rs::mentions_type_parameter_inner` currently follows
   `peek_parameter_type`, which omits unpublished symbol slots. A full lazy
   cutover must not turn an unpublished edge into a proven absence of type
   parameters. The calls owner owns this walk, but its `&self` contract is
   consumed outside ownership by `contextual.rs`, `declared.rs`,
   `index_access_reports.rs`, `mapped.rs` and `members.rs`. Agree on demand or
   prepared-completion semantics before changing those callers. Simply
   ignoring the edge can incorrectly skip instantiation/inference.
5. **Oracle coordination:** `parity-full-corpus` exclusively owns all
   `crates/tsr-conformance/src`, including `types_producer.rs`, and owns the
   migration of obsolete expected-driven producer API callers. Route any
   producer contract request through the integrator; this lane must not edit
   conformance sources or other owners' permanent caller tests. New
   `calls_lazy_*.rs` tests remain exclusive to this lane. The current coverage
   runner writes snapshots unconditionally; the oracle owner/integrator must
   provide or run the read-only full-population gate. The existing verdict
   tools are not that gate. Native root claims `tsr-2zk.16.27` and
   `tsr-2zk.16.61` are assigned; this lane does not claim their work.

Once these contracts are available, the calls owner can convert original
parameter construction, all owned parameter printers and mapping/inference
consumers together. Pattern parameters currently use eager declaration
computation and admission gates; a whole-parameter-symbol port must also
coordinate their binder identities rather than retaining an undocumented
identifier-only cutover.

## Frozen baseline and verification limits

Before any tracked edit, the unfiltered commands completed:

```text
cargo build --release -p tsr-conformance -p tsr
cargo run -q --release -p tsr-conformance --example diagverdictdump
cargo run -q --release -p tsr-conformance --example verdictdump
```

The frozen release binary and dumps were stored under `/tmp/box/base`.
Their SHA-256 identities:

- `tsr`: `866e1c6ebe89bf98af5cd20e04fb44b93bb4f73a4cd3d84566c6021c0893211b`
- `diag.tsv`: `ff08a03bb448a639ac7e219d8e7e4b2921b6aa5cf95230c4744efba413da9482`
- `types.tsv`: `25ed9bd55f95956c4515044bf7f41334799bbcc6e2a186eaf6ad36824621660a`
- Native CLI: `7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`

Diagnostic dump: 10,570 rows; RIGHT 4,221, EMPTY_RIGHT 4,968, WRONG 1,281,
EMPTY_WRONG 100. Type dump: 477,970 verdict rows; RIGHT 469,765, GAP 993,
WRONG 7,212. The type file also contains six non-verdict continuation lines;
raw physical line count is 477,976, not the semantic denominator.

These are unfiltered *tool invocations*, not full-population proof:
`verdict.rs::verdict_rows` excludes varied types and known divergences, and
omits absent/unaligned producer assertions. Diagnostic verdicts do not prove
complete messages, lengths, chains and order. No candidate binary exists,
so no after-loss check, candidate performance claim or verified TSR/tsgo
ratio is asserted. No code tests were added or run for a nonexistent port.

Named targets checked against current code:

- `compiler/functionWithDefaultParameterWithNoStatements16`: diagnostics
  empty; focused type dump contains no WRONG/GAP rows. Not a new conversion.
- `compiler/contextualParamTypeVsNestedReturnTypeInference4`:
  EMPTY_RIGHT diagnostics. Not a new conversion.
- `compiler/reverseMappedTypeContextualTypeNotCircular`: WRONG diagnostics;
  TSR emits none. Pinned native directly reports TS2322 with
  `Target signature provides too few arguments. Expected 2 or more, but got 1.`
  This observed missing relation diagnostic is not evidence of current
  eager-parameter circularity in that case.

Converted cases: none. The release target and all remaining exclusive
cases remain unverified, not reduced to the controls above.
