# External Integrations

**Analysis Date:** 2026-08-03

## Overview

This codebase is a Rust port of the TypeScript compiler's scanner and parser. **It has no external service dependencies, APIs, databases, or webhooks.** It is a standalone library and CLI tool that operates entirely on local files and memory.

## File I/O Only

**Input:**
- TypeScript source files (as byte streams)
- Conformance corpus: `vendor/typescript-go/testdata/` (12,444 test cases)
- Upstream schema: `vendor/typescript-go/_scripts/ast.json` (used by `cargo xtask codegen`)

**Output:**
- Console diagnostic messages (to stderr)
- Conformance snapshots: `crates/tsr-conformance/snapshots/*.snap` (committed)
- Generated code: `crates/tsr-ast/src/generated/` (committed)

## Upstream Relationship

**Source of Truth:** `vendor/typescript-go` (Git submodule)
- URL: `https://github.com/microsoft/typescript-go.git`
- Pinned commit: `5b1047d10`
- Relationship: Read-only conformance oracle
- Size: ~300k LOC (unchanged)

**Not a runtime dependency** — the submodule is used only for:
1. Code generation (AST schema)
2. Conformance testing (baseline corpus)

The submodule can be absent for a plain `cargo build` (generated code is checked in). Tests that read the submodule skip gracefully when absent.

## Authentication & Secrets

**None required.** No API keys, tokens, or credentials are needed to build, test, or run the codebase.

## Databases

**Not used.** All state is in-memory (arena allocator + side tables).

## Storage

**Local filesystem only:**
- Reads: TypeScript source files provided as input
- Writes: Conformance snapshots and generated code (both committed to git)

## Monitoring & Observability

**None configured.** No error tracking, remote logging, or telemetry.

## CI/CD Specifics

**CI Platform:** GitHub Actions

**No external secrets required** — GitHub Actions workflow (`.github/workflows/ci.yml`) runs on `ubuntu-latest` with no environment variables except:
- `CARGO_TERM_COLOR=always` (local formatting only)
- `RUSTFLAGS=-D warnings` (compiler option)

**No artifact uploads** — all build artifacts are cached locally via `Swatinem/rust-cache@v2`.

## Package Registry

**Upstream:** `registry+https://github.com/rust-lang/crates.io-index`

All dependencies are pulled from the public Rust crates.io registry at build time. No private registries or authentication.

## Summary

| Aspect | Status |
|--------|--------|
| External APIs | None |
| Databases | None |
| Authentication / Auth Providers | None |
| Webhooks | None |
| Cloud Services | None |
| CDNs | None |
| Monitoring / Telemetry | None |
| Environment Configuration | Not applicable (no external services) |

The project is self-contained and requires only a Rust toolchain and the git submodule.

---

*Integration audit: 2026-08-03*
