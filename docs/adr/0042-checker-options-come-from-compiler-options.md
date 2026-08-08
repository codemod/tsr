# 0042 — Checker options come from `CompilerOptions`, once

**Status:** accepted
**Date:** 2026-08-08
**Supersedes:** nothing. Narrows the ad-hoc reading that grew alongside
[ADR-0040](0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md).

---

## The forcing constraint

`Checker` reads eleven compiler options. Until this decision, **no caller passed
it a `CompilerOptions`** — each derived eleven booleans itself, from the
conformance corpus's raw `@directive` string map, and wrote them in through
eleven setters.

Two callers did this. They disagreed, and the disagreement was durable rather
than transient:

| option | `diagnostics_suite` | `types_producer` | upstream |
|---|---|---|---|
| `noImplicitAny`, unset | **true** | **false** | true |
| `useUnknownInCatchVariables`, unset | **true** | **false** | true |
| `strictPropertyInitialization`, unset | fell back through `strictNullChecks` | not set at all | falls back to `strict` only |
| `isolatedModules` | read from the raw map | dropped (`bd tsr-e7a`) | a declared option |
| `verbatimModuleSyntax` | not read | not read | implies `isolatedModules` |

The `noImplicitAny` row was known and recorded: `checker-notes-diag2.md` §80
observes that the two lines "disagreed about one question for eleven sessions".
It was not resolved, because each side had a *measurement* justifying it —
`types_producer`'s `false` default was chosen because `true` scored worse on the
`.types` baselines (§21 and §65), and `diagnostics_suite`'s `true` was chosen
because it is what upstream does.

This is the shape of the real constraint. The rules are not uniform, and which
family an option belongs to is not guessable from its name:

| how upstream reads it | unset means | example |
|---|---|---|
| `GetStrictOptionValue` (`core/compileroptions.go:294`) | **on**, unless `strict: false` | `strictNullChecks` |
| `IsTrue()` | off | `noUnusedLocals` |
| `IsTrueOrUnknown()` | on, unconditionally | `noUncheckedSideEffectImports` |
| two reads of one option | see below | `allowUnreachableCode` |

`allowUnreachableCode` needs both `IsTrue()` and `IsFalse()`, and they are not
each other's negation: unset makes unreachable code a *suggestion*, which never
reaches an `.errors.txt` (`checker-notes-diag2.md` §82). Any caller re-deriving
this from scratch has four chances to pick the wrong default and one chance to
collapse a three-valued option into a boolean.

And the CLI made a third caller imminent. Three independent derivations of a rule
that two could not agree on is not a risk, it is a schedule.

## The decision

1. `CompilerOptions` grows the ten fields the checker reads and did not have —
   `strict_null_checks`, `strict_property_initialization`,
   `use_unknown_in_catch_variables`, `no_unchecked_indexed_access`,
   `no_unused_locals`, `no_unused_parameters`, `allow_unreachable_code`,
   `preserve_const_enums`, `verbatim_module_syntax`,
   `no_unchecked_side_effect_imports` — as `Tristate`, **unresolved**.
2. The three upstream accessors are ported onto it:
   `strict_option_value` (`GetStrictOptionValue`), `get_isolated_modules`, and
   `should_preserve_const_enums`.
3. `Checker::apply_compiler_options` resolves all eleven in one place, porting
   `NewChecker`'s option block (`internal/checker/checker.go:915-928`) plus the
   three options upstream reads lazily at their use sites.
4. Both conformance callers call it. The eleven setters remain, because unit
   tests in `tsr-checker` construct a checker with no program and therefore no
   options, but no *suite* uses them.

The `Tristate` stays unresolved in `CompilerOptions` deliberately. Storing the
resolved `bool` would move the defaulting rule back to whoever filled the struct,
which is the problem this decision exists to remove.

## The alternatives, taken seriously

**Keep per-caller derivation and add a test asserting the two agree.** Rejected:
a test that two derivations agree still leaves two derivations, and the thing
they must agree with is upstream, not each other. It would also have failed on
day one, since they genuinely disagreed and one of them was scoring better for it.

**Pass `CompilerOptions` to `Checker::new` and read fields lazily at use sites,
as upstream does.** This is the more faithful shape and is where the port should
eventually land. Rejected *for now* on cost: `Checker` holds eleven `bool` fields
consulted in hot paths, and threading an options reference through construction
touches every one of the ~15 `Checker::new` call sites in tests. The resolution
logic — the part that was actually wrong — is centralised either way, so the
cheap version buys the entire correctness benefit. **This would win if a future
option needed to be re-read after construction**, which none of these eleven do.

**Resolve into `CompilerOptions` at parse time**, so the struct holds booleans.
Rejected: `strict` is itself an option, so "resolve at parse time" means the
config parser has to know the strict family — and a `tsconfig.json` that sets
`strict` after `strictNullChecks` would resolve differently from one that sets it
before. `Tristate` exists precisely so that order does not matter.

## The consequences accepted

**Two behaviours changed, and both were measured.**

Consolidating the eleven reads in `diagnostics_suite` moved **nothing** —
every suite identical, including the corrected `strictPropertyInitialization`
fallback and the newly-honoured `verbatimModuleSyntax`. A pure fidelity fix with
no conformance signal, which is the expected shape for a rule the corpus does not
exercise.

Applying the same derivation to `types_producer` **overturned a recorded
measurement**: `checker_types` went 3,842 → **3,863** cases (+21) and 84.13% →
**84.14%** of lines. The `false` defaults that §21 and §65 measured were true of
the checker that produced them; with `catch (e)` now typed `unknown` and
un-annotated parameters implicitly erroring, the baselines those defaults were
compensating for render correctly without them.

The general lesson is narrower than "measure less", and is worth stating because
this project measures a great deal: **a default tuned against a partial
implementation measures the gap, not the language.** It has to be re-measured
whenever the gap closes. An unfaithful setting that scores better is a marker for
an unported rule somewhere else, not a finding about TypeScript.

**`isolatedModules` is now honoured as a test directive**, closing `bd tsr-e7a`.
It was dropped from `apply_test_directives` on the grounds that it belonged to
the declaration-emit suites; that was half true, since `diagnostics_suite` read
it privately out of the raw map because `ShouldPreserveConstEnums` folds it in.
Routing every checker option through `CompilerOptions` left nowhere for that
private read to live.

**Accepted cost: eleven setters that no suite calls.** They are still used by
`tsr-checker`'s own unit tests, which construct a checker with no program. They
should go when those tests can build a `CompilerOptions` cheaply.

## How we would know this was wrong

- **A checker option that must change after construction.** Every one of the
  eleven is read at construction here; watch mode or a language service that
  re-configures a live checker would need the lazy shape instead, and this
  decision would be the thing in the way.
- **`checker_types` or `diagnostics` regressing on a later re-measure while a
  hand-tuned default would have held.** That would mean the faithful reading is
  not yet the best available and the gap it exposes is larger than the gap it
  closes — in which case the deviation should be reinstated *explicitly, with its
  number*, not by letting a caller derive its own.
- **A third read family appearing** that `apply_compiler_options` cannot express
  without a caller-supplied override.
