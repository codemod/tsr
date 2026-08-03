# ADR-0007: Classify generated code by source of truth, and port only Category A

**Status:** Accepted
**Date:** 2026-08-03
**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`

## Context

typescript-go checks in **28 generated non-test Go files** (plus 6,568 generated
fourslash tests). They total roughly 46k LOC and are produced by six different
generators. A port has to decide, for each one, whether to port the generator,
port the output, or do neither — and getting that wrong in either direction is
expensive:

- Porting too little means hand-maintaining data that upstream regenerates,
  which is how a conformance gap opens silently.
- Porting too much means writing generators for things Rust already does, and
  carrying a code generator forever to produce what a `derive` gives free.

Naively, "it's generated upstream, so generate it here" looks like the safe rule.
It is not: roughly a third of these files exist purely to work around a Go
language limitation.

## Decision

Classify every generated artifact by **what its source of truth is**, and port
only Category A.

### Category A — derived from external data

The input is a data file outside the Go source: TypeScript's own definitions,
Unicode tables, the LSP meta-model. The data will change when upstream updates,
and nothing in Rust supplies it.

**Port the generator into `xtask`, reading the same input file.** Never transcribe
the output by hand.

### Category B — derived from the Go source itself

The input is Go code, and the generator exists because Go lacks a language
feature. `stringer` is `#[derive(Debug)]`. `moq` is a mock library. Neither
carries information; both are mechanical restatements of a declaration.

**Do not port.** Use the Rust feature. There is nothing to keep in sync, because
the source of truth is our own code.

## The register

Status is as of this ADR; keep it current.

| Upstream artifact | LOC | Source of truth | Cat. | Our status |
|---|---:|---|:--:|---|
| `ast/ast_generated.go` | 10,051 | `_scripts/ast.json` | A | **Done** — `xtask/gen_nodes.rs` |
| `ast/kind_generated.go` | 463 | `_scripts/ast.json` | A | **Done** — `xtask/gen_kind.rs` |
| `diagnostics/diagnostics_generated.go` | 8,626 | `diagnosticMessages.json` + `extraDiagnosticMessages.json` | A | **Done** — `xtask/gen_diagnostics.rs` |
| `diagnostics/loc_generated.go` + `loc/*.json.gz` | 151 | Microsoft LCX translations, 13 locales | A | **Not built** — `bd tsr-5e7.6` |
| `stringutil/identifier_parts_generated.go` | 1,391 | Unicode 15.1.0 ID_Start / ID_Continue | A | **Done** — `xtask/gen_unicode.rs` |
| `stringutil/js_case_generated.go` | 3,496 | Unicode 15.1.0 case mappings + Final_Sigma | A | **Not built** — needed by the checker, not the scanner |
| `bundled/libs_generated.go`, `embed_generated.go` | 568 | `lib.*.d.ts` from the TypeScript submodule | A | **Not built** — needed from P3 |
| `lsp/lsproto/lsp_generated.go` | 17,466 | LSP `metaModel.json` | A | **Not built** — P8. Evaluate `tower-lsp-server` first; it may supply this |
| `api/encoder/{encoder,decoder}_generated.go` | 1,847 | `_scripts/ast.json` | A | **Not built** — needed for the API surface |
| `ast/kind_stringer_generated.go` | 375 | the Go `Kind` enum | B | **Not ported** — generated `SyntaxKind::name()` |
| 10 further `*_stringer_generated.go` | ~320 | their Go enums | B | **Not ported** — `#[derive(Debug)]` / hand-written `name()` |
| 3 `*mock_generated.go` (moq) | 1,259 | Go interfaces | B | **Not ported** — hand-written fakes |
| `fourslash/tests/gen/*_test.go` | — | fourslash `.ts` test files | A | **Not built** — P7, needs the fourslash DSL |

## Rules that follow

These apply to every Category A generator we write.

1. **Read upstream's input, not upstream's output.** The one exception is the
   Unicode tables, and it is justified in-line below.
2. **Fail loudly on anything unrecognised.** No permissive fallback. An earlier
   version of the AST generator degraded unmapped types to an opaque `Node` and
   silently hid 15 token-alias types.
3. **Reproduce load-bearing strings byte-for-byte; own everything else.** A
   diagnostic *key* (`_0_expected_1005`) is upstream's data and is reproduced
   exactly. The Rust *identifier* is ours, and bends where the two conflict.
4. **Assert conformance against upstream's generated output**, never against the
   input we share with it ([ADR-0006](0006-conformance-oracle.md)). Testing the
   generator against its own input proves only self-consistency.
5. **Check the output in**, so a plain `cargo build` works without submodules,
   and have CI regenerate and diff it.
6. **Deviate deliberately, and record it.** Where upstream's generator is
   non-deterministic or lossy, we may diverge — but the divergence must be
   quantified by a test. See below.

## Deliberate divergences so far

**Diagnostics are a strict superset (2,162 vs. upstream's 2,154).** Eight
diagnostic codes are shared by two messages each. Upstream's `readRawMessages`
builds a `map[int]*diagnosticMessage` keyed by code, so one of each pair is
dropped — and because Go map iteration is randomised, *which* one is dropped
varies between regenerations. We keep both: no information is lost, the output is
byte-stable, and messages are properly identified by key rather than code.
`message_conformance.rs` asserts the superset relation is exactly those 8.

**Unicode tables are transcoded from upstream's generated Go, not from the
Unicode data.** This is the one violation of rule 1, and it is deliberate:
upstream generates from the `@unicode/unicode-15.1.0` npm package, which is not
vendored. Transcoding the vendored Go tables pins us to *exactly* TypeScript's
Unicode version, which is what identifier-scanning conformance requires. Using the
`unicode-id-start` crate instead would silently track a different Unicode version
and diverge on rare code points — precisely the kind of gap that surfaces years
later as one mysterious failing conformance case.

## How we would know this was wrong

If a Category B judgement turns out to carry information we then have to
reconstruct by hand. The likeliest candidate is `lsp_generated.go`: it is
classified A, but if `tower-lsp-server` already models the protocol, generating
17k LOC from `metaModel.json` would be waste — that entry should be re-examined
before P8 rather than executed on schedule.
