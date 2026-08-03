# Documentation

Reasoning lives here. `PLAN.md` at the repo root holds the roadmap — scope,
phases, and gates — and links into this directory rather than duplicating it.

## Decision records — `adr/`

Numbered, one decision per file, immutable once merged. A decision that no longer
holds is *superseded* by a new record that references it, never edited or deleted.

| # | Decision | Status |
|---|---|---|
| [0001](adr/0001-idiomatic-rewrite.md) | Idiomatic Rust rewrite, not a mechanical transliteration | Accepted |
| [0002](adr/0002-own-ast.md) | Build our own AST matching TypeScript's shape, not `oxc_ast` | Accepted |
| [0003](adr/0003-tree-plus-side-tables.md) | The tree is a tree; everything cyclic lives in id-keyed side tables | Accepted |
| [0004](adr/0004-oxc-inspiration-not-dependency.md) | Take oxc's designs and dependency choices; link no `oxc_*` crate | Accepted |
| [0005](adr/0005-codegen-from-ast-json.md) | Generate the AST from upstream's `_scripts/ast.json` | Accepted |
| [0006](adr/0006-conformance-oracle.md) | Assert conformance against generated Go, not against `ast.json` | Accepted |
| [0007](adr/0007-generated-code-policy.md) | Classify generated code by source of truth; port only Category A | Accepted |
| [0008](adr/0008-jsdoc-parsed-eagerly.md) | Parse JSDoc during the main parse; upstream's lazy path needs a lifetime we do not have | Partly superseded by 0010 |
| [0009](adr/0009-performance-gate.md) | Gate performance against typescript-go at the pin, not against our own history | Proposed |
| [0013](adr/0013-checker-memoisation.md) | The checker computes through `&mut self` and returns handles, not references; no interior mutability | Accepted |
| [0012](adr/0012-ast-is-sync.md) | Nodes hold no interior mutability, so a parsed tree can be shared across threads | Accepted |
| [0011](adr/0011-unsafe-is-opt-in.md) | `unsafe_code = "deny"` workspace-wide; three justified exceptions | Accepted |
| [0010](adr/0010-jsdoc-is-a-parse-option.md) | JSDoc becomes a parse option; the gate's ratio is measured without it, as upstream does | Accepted |

## Architecture — `architecture/`

Living documents describing how a subsystem works and why it is shaped that way.

- [ast.md](architecture/ast.md) — node representation, ids, side tables, codegen
- [scanner.md](architecture/scanner.md) — tokenisation, re-scanning, backtracking
- [parser.md](architecture/parser.md) — recursive descent, recovery, the arrow-function trap
- [jsdoc.md](architecture/jsdoc.md) — two languages in one file, and switching between them
- [performance.md](architecture/performance.md) — the tsgo comparison, and where the time goes
- [threading.md](architecture/threading.md) — what is `Send`, what is not, and why
- [conformance.md](architecture/conformance.md) — the corpus, the oracles, the ratchet

## Conventions

- [conventions.md](conventions.md)

## A note on numbers

Claims about upstream are anchored to the pinned submodule commit
(`vendor/typescript-go` @ `5b1047d10`). Upstream moves; a measurement without a
pin is not reproducible. Where a figure here was later found to be wrong, the
correction is recorded inline rather than silently applied — see the kind-count
note in [ast.md](architecture/ast.md).
