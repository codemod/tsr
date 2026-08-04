# `isolatedDeclarations` analysis (`tsr-dts`)

**Status:** slice 1 of Phase 3.5 (`bd tsr-49v.2`). The analysis exists; the printer
and the `.d.ts` text do not (`bd tsr-49v.4`).

**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.

This document is about *how the analysis is shaped and what it is measured
against*. The decision to write it rather than port it lives in
[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md) and is not restated
here.

## What it does

`tsr_dts::analyze(source_file, nodes) -> Vec<Diagnostic>` answers one question per
declaration: **would emitting this into a `.d.ts` have required inferring a type?**
If so it reports the corresponding `TS9xxx` diagnostic. It never produces text.

Two passes, both purely syntactic:

| Pass | Module | Question |
|---|---|---|
| Visibility | `visibility.rs` | which top-level declarations reach the `.d.ts` at all? |
| Rules | `rules.rs` | of those, which would need inference? |

## Anchoring, for a component with no upstream counterpart

ADR-0021 replaced the usual rule (name your typescript-go counterpart) with a
narrower one, because there is no counterpart to name:

1. Each rule names the `TS9xxx` **code** it implements and the upstream message
   constant in `internal/diagnostics/`.
2. The drift tracker watches `internal/diagnostics/` for the `TS9xxx` range, **not**
   `internal/transformers/declarations/`.
3. Where a rule takes oxc's shape, it says so by file.

## The part that is harder than the rules: where the squiggle goes

A diagnostic with the right code at the wrong position fails a case exactly as
completely as a missing one. The anchors are not derivable from the messages and
were read off the baselines one at a time; the table lives in `rules.rs`'s module
documentation, next to the code that implements it, so the two cannot drift apart.

The organising principle is worth stating here because it shapes the API. Failure
is three-valued, not boolean:

- **`Ok`** — the type is apparent.
- **`Reported`** — it is not, and a *specific* diagnostic (`TS9017` for a mutable
  array, `TS9015` for a spread, …) has already been filed at the offending node.
- **`Generic`** — it is not, and only the caller knows what to say: `TS9010` at a
  variable's name, `TS9012` at a property's name, `TS9011` at a parameter's
  initialiser, `TS9013` at the sub-expression when the failure is nested inside a
  literal.

A specific diagnostic *replaces* the generic one rather than joining it:
`export let arr = [1, 2, 3];` produces `TS9017` at the array and no `TS9010` at the
name.

## Two findings that changed the rules

### The corpus's own comments are wrong about function expressions

`isolatedDeclarationErrorsReturnTypes` marks a block of exported
`const fn = function foo() { return 0; }` declarations `// Should Error`, and a
block of class fields likewise. **Neither errors.** Not in typescript-go's baseline,
and not in TypeScript's own — the two files are identical, 31 diagnostics, the first
at line 13. Every `TS9007` in that 170-line file is anchored on a function used as a
**parameter default**.

Implementing what the comments said cost 54 false positives in that one case, which
is how it was found. The rule is now scoped to parameter defaults.

The one apparent counter-example is checker-driven and deliberately not matched:
in `isolatedDeclarationErrors.ts` an arrow *does* get `TS9007`, but only because an
expando assignment (`errorOnMissingReturn.a = ""`) makes upstream build the
function's type through `IsExpandoFunctionDeclaration`.

### `as const` is a `TypeReferenceNode` with no name

`const` is a keyword, so it never becomes an identifier; the parser gives a const
assertion a `TypeReferenceNode` whose `type_name` is `None`
(`crates/tsr-parser/src/types.rs:403`). Matching on the name `"const"` therefore
matched nothing, and every `as const` was silently read as an ordinary `as T` — an
*explicitly stated* type, so nothing inside one was ever examined. It failed
silently in the safe-looking direction and was caught by a unit test, not by the
corpus. Both spellings are matched now.

## The oracle, and what it can and cannot see

The gate is the `isolated_declarations` conformance suite: the `TS9xxx` diagnostics
of every case that sets `@isolatedDeclarations: true`, compared **by code and
position** against upstream's `.errors.txt`.

Measured at the pin:

| | |
|---|---|
| Corpus cases setting `@isolatedDeclarations: true` | **21** |
| Judged (after exclusions) | **15** |
| Excluded: known divergence (`.errors.txt.diff`) | 5 |
| Excluded: upstream recorded no baseline | 1 |
| Positioned `TS9xxx` in those baselines | **172**, across 20 distinct codes |

> **Correction to the record.** ADR-0021 states 22 cases. The corpus the harness
> enumerates — the TypeScript submodule's `compiler/` and `conformance/` — contains
> **21**. The 22nd was almost certainly `isolatedDeclarationsTypePredicate`, which
> lives in typescript-go's *own* `testdata/tests/cases/compiler/` and has no
> submodule counterpart, so `Corpus::discover` never sees it. There are five such
> tsgo-local cases; they are a real oracle nobody reads yet (`bd tsr-49v.6`).

Only the **header block** of an `.errors.txt` is parsed. A baseline states every
diagnostic twice — once positioned in the header, once as a `!!!` line under the
source echo — so a naive `grep -c` returns exactly double and every count comes out
suspiciously even. `crates/tsr-conformance/src/errors_baseline.rs` documents this
and tests it.

Diagnostics outside the `9000..9100` range are filtered out of the comparison.
Three of the sixteen baselines mix ordinary checker errors in with the `TS9xxx`
ones (12 in total); those are Phase 4's, and comparing whole baselines would make
this suite unpassable for reasons unrelated to what it measures.

### The gate is coarse at this pass rate, and the mutations say so

Four mutations were applied to the code under test and re-measured over the corpus.
**None of them moved the pass rate**, because a case that already fails on an
exact-match comparison keeps failing when made worse. What moved was the snapshot's
per-case detail:

| Mutation | Pass rate | Snapshot lines changed |
|---|---|---|
| `TS9010` anchored at the initialiser instead of the name | 5/15 | 6 |
| `private` / `#` members judged like any other | 5/15 | 2 |
| Parameter defaults not treated as a special position for `TS9007` | 5/15 | 2 |
| **Visibility pass disabled entirely** | 5/15 | **0** |

Two things follow, and both are load-bearing:

1. **Read the snapshot diff, not the rate.** Until the rate is high, the rate is
   the insensitive instrument. This is the same reason the project's method says to
   diff the failure list by case name.
2. **The visibility pass is not currently tested by the corpus at all.** Disabling
   it changes nothing, because the rules that would have fired on invisible
   declarations are exactly the `TS9007`-on-initialiser rules that the first finding
   above removed. Its only teeth are the unit tests in
   `crates/tsr-dts/tests/rules.rs`, which do fail under that mutation. It is kept
   because it is demonstrably right and will become load-bearing the moment the
   `.d.ts` output oracle exists — but until then it is a pass whose correctness the
   gate cannot see, and that should be said out loud rather than assumed.

## Known approximations

Each of these is a place where upstream consults the checker and this analysis
guesses. They are the expected source of divergence under ADR-0021.

| Area | Upstream | Here |
|---|---|---|
| Visibility | `EmitResolver.IsDeclarationVisible` | reachability from the exports by top-level name; no scope chain, no declaration merging |
| Computed property names (`TS9038`) | the name expression's *type* — string/number literal, or `unique symbol` | literals and dotted names pass. `[str]` where `str: string` is an error upstream and accepted here |
| Expando assignments (`TS9023`) | `IsExpandoFunctionDeclaration` | a top-level assignment to a property of a top-level name; cannot tell a function from any other binding |
| Enum initialisers (`TS9020`) | constant-expression evaluation | literals and arithmetic over them; a bare identifier is accepted, since it is nearly always an earlier member of the same enum |

Reference collection for visibility descends into everything except function
bodies, which over-approximates: a declaration may be judged that upstream would
have dropped. The direction is deliberate — an over-approximation reports a
diagnostic upstream does not and shows up as a conformance failure, while an
under-approximation stays silent.

## What is not done

The remaining failure buckets, by root cause, are filed as children of
`bd tsr-49v.2`. The largest are the accessor-pair anchor (`TS9009`), enum
initialisers (`TS9020`), computed names (`TS9038`), and the four codes with no
implementation at all: `TS9021`, `TS9022` in heritage position, `TS9025`
(implicitly-added `undefined`), and `TS9026` (augmentation imports).
