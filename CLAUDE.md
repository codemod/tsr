# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:7510c1e2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->

## Documentation — read this before writing code

**Everything gets documented in `docs/`.** This project is a multi-year port of a
300k-LOC compiler. The code will outlive everyone's memory of why it looks the way
it does, and an idiomatic rewrite (rather than a transliteration) means the "why"
is *not* recoverable by diffing against upstream. Undocumented reasoning is lost
reasoning.

### The rule

Any change that involves a judgment call gets its reasoning written down in
`docs/` **in the same commit as the code**. Not afterwards, not in the PR
description, not only in the commit message.

A judgment call means: choosing between viable alternatives, rejecting the obvious
approach, deviating from upstream's structure, accepting a known limitation, or
discovering something non-obvious about upstream.

### Where things go

| Location | Contents |
|---|---|
| `docs/adr/` | **Decision records.** One file per decision, numbered, immutable once merged. Superseded by writing a new one that references it. |
| `docs/architecture/` | **How a subsystem works and why it is shaped that way.** Living documents; edit in place. |
| `docs/conventions.md` | Cross-cutting rules every crate follows. |
| `PLAN.md` | The roadmap: scope, phases, gates. Links into `docs/`; does not duplicate it. |
| Rustdoc | Behaviour of *this* item. Links to `docs/` for the wider rationale. |

### Write from first principles

Documentation that only records *what* was decided is nearly worthless six months
on. Record the reasoning chain:

1. **The forcing constraint.** What about the problem made this necessary? Prefer
   measured facts over assertions — "upstream reads `.Parent` 2,092 times in
   `checker`/`ls`/`binder`" beats "parent access is hot".
2. **The alternatives, taken seriously.** Including the one you rejected. State
   what would have to change for the rejected option to win — that is what makes
   a decision revisitable rather than merely historical.
3. **The consequences you accepted**, including the bad ones.
4. **How you would know you were wrong.** A decision with no falsifier is a
   preference.

Cite evidence by path and, where it matters, by line or count:
`vendor/typescript-go/internal/core/linkstore.go`. Anchor claims about upstream to
the pinned commit, since upstream moves.

### Non-negotiables

- **Never delete a decision record.** Supersede it. The wrong turns are the most
  useful part of the archive.
- **Never document an intention as though it were built.** If it does not exist,
  say so and link the `bd` issue. Aspirational docs are how a project loses track
  of what it actually has.
- **Correct the record when a number turns out to be wrong**, and note that it was
  corrected. Silent edits destroy trust in every other number.

## Build & Test

```bash
git submodule update --init --recursive   # required for codegen and conformance

cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all

cargo xtask codegen && cargo fmt --all    # regenerate the AST
cargo run -p tsr-conformance --bin coverage   # conformance run; writes snapshots
```

Generated code under `crates/tsr-ast/src/generated/` is checked in and must never
be hand-edited. CI regenerates and diffs it.

## Architecture Overview

A Rust port of `microsoft/typescript-go`, pinned as a submodule at
`vendor/typescript-go`. See [PLAN.md](PLAN.md) for scope and phasing, and
[docs/architecture/](docs/architecture/) for how each piece works.

The one-line version: **the AST is a tree with no back-edges, and everything
cyclic — parent, symbol, scope, type, flow node — lives in id-keyed side tables.**
See [docs/adr/0003-tree-plus-side-tables.md](docs/adr/0003-tree-plus-side-tables.md).

## Conventions & Patterns

See [docs/conventions.md](docs/conventions.md). The two that bite hardest:

- **Anchor ported code to upstream.** Every ported item names its typescript-go
  counterpart in a doc comment. This is what makes upstream drift trackable.
- **Conformance is asserted against generated Go, not against `ast.json`.** Those
  two sources do drift. See
  [docs/adr/0006-conformance-oracle.md](docs/adr/0006-conformance-oracle.md).
