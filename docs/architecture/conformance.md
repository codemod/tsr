# Conformance

**Crate:** `tsr-conformance` (library + `coverage` binary)
**Snapshots:** `crates/tsr-conformance/snapshots/*.snap`, committed
**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`

```bash
cargo run -p tsr-conformance --bin coverage
```

## Why this exists before the compiler does

The harness was built before the scanner and parser deliberately. Without it there
is no denominator, so "the parser is going well" is unfalsifiable. With it, every
stage has a committed number that can only be moved by making the compiler better.

The governing rule: **a stage that does not exist reports 0%, loudly.** It does not
skip, and it does not vanish from the summary. In a table, a missing row and a zero
row look the same, and only one of them tells you where you are.

## Current numbers

```
suite                                    passed      rate   skipped
corpus_ingest                       12444/12444   100.00%         0
baseline_resolution                 12444/12444   100.00%         0
parser_reachable_target              5648/11187    50.49%      1257
scanner_termination                 12444/12444   100.00%         0
scanner_clean_files                   5648/5648   100.00%      6796
parser_typescript                     5376/5648    95.18%      6796
```

The first two measure the **harness**; `parser_reachable_target` measures the
**size of the target**; the scanner suites and `parser_typescript` measure the
**compiler**.

`scanner_clean_files` was the first suite to find real bugs — three of them, none
visible by inspection. See [scanner.md](scanner.md). `parser_typescript` then found
an exponential blowup in arrow-function lookahead that ran the parser to 16 GB; see
[parser.md](parser.md).

### Driving the scanner without a parser

`scanner_clean_files` cannot use a bare `while scan()` loop: templates and regular
expressions need context the scanner does not have, and a naive loop blames the
scanner for its own mis-driving. Before this was fixed the suite read 96.55%, of
which most failures were the harness's fault.

The suite therefore tracks template substitution nesting exactly (decidable) and
*probes* each `/` as a regular expression, rewinding via `Scanner::restore` when it
does not close. The probe is a heuristic — it would mis-handle some division
expressions — which is acceptable for a "produces no diagnostics" check and would
not be for a token-stream comparison. It is harness scaffolding; the parser
replaces it.

## The corpus

- **Cases:** `vendor/typescript-go/_submodules/TypeScript/tests/cases/{compiler,conformance}`
  — 12,444 `.ts`/`.tsx` files. `fourslash` (6,568 files) and `project` are excluded:
  they are not file-in/baseline-out compiler tests and need their own runners.
- **Baselines:** `vendor/typescript-go/testdata/baselines/reference/submodule/{compiler,conformance}`
  — flat directories keyed by case basename, 45,775 files in total.

### The load-bearing convention

**A missing `.errors.txt` means "no diagnostics expected."** Upstream writes that
baseline only when a case produces errors.

That asymmetry is the entire reason a parser can be measured before a checker
exists: for the 5,648 cases with no `.errors.txt`, the expected diagnostic output
is exactly *nothing*, and a parser can be held to that. Cases that *do* expect
errors need the checker, because `.errors.txt` mixes syntactic and semantic
diagnostics with no marker distinguishing them.

### Configuration-varied baselines — the trap

A case declaring `// @target: es5, es2015` is compiled once per configuration, and
each run writes its own baseline:

```
accessorInAmbientContextES5(target=es2015).errors.txt
```

**793 cases have only varied baselines and no plain `.errors.txt`.** Resolving by
exact name alone reads those as "expects no diagnostics" — inflating the reachable
target by 793 and, worse, giving a future parser 793 free passes for emitting
nothing.

This was a real bug in the first version of the harness. It was caught by
cross-checking the harness's count against a direct `find` over the baseline
directory (7,027 `.errors.txt` on disk vs. 5,539 the harness had attributed), not
by any test. `BaselineIndex::has_variant` now matches `<stem>(...).<ext>`, and
`corpus.rs` has regression tests covering it — including that `a` must not match
`ab(target=es5).errors.txt`.

Full per-configuration runs are not implemented; those cases are currently
*skipped*, and the skip count is printed so the exclusion is never silent.

### Cases with no recorded output

**617 cases have no baseline of any kind** — no `.errors.txt`, `.types`,
`.symbols`, `.js`, or `.diff`. Upstream never ran them.

The load-bearing convention above says a missing `.errors.txt` means "expects no
diagnostics", but that inference only holds when *some* baseline proves the case
was run at all. Without one, the absence proves nothing: `exportNonInitializedVariablesAMD`
opens with `var; let; const;`, which TypeScript rejects outright, yet it has no
recorded errors.

Counting those as clean expectations inflated the denominator and handed free
passes to whatever the parser happened to accept. They are now skipped, which cost
about 600 apparent passes and is the honest number.

This is the configuration-varied-baseline trap in a second guise: an absent file
is not evidence, and only a positive signal that the case *was* run makes its
absence meaningful.

### Known divergences

typescript-go writes `.diff` baselines where its own output intentionally differs
from the TypeScript reference (1,208 in these two suites). Those cases cannot be
judged against TypeScript's expectations without accounting for the divergence, so
they are skipped and counted.

## The case parser

`case.rs` mirrors `ParseTestFilesAndSymlinksWithOptions`
(`internal/testrunner/test_case_parser.go`) rather than approximating it. The rules
that matter, each with a test:

- Directives match `^//\s*@(\w+)\s*:\s*(.*)$`, **anchored at line start** — an
  indented `// @filename:` inside a code block is ordinary source.
- Option names are lowercased; values trimmed.
- `@filename` splits units. `@currentDirectory` and `@symlink` are not options.
- Newline is a **separator**, so a unit closed by the next directive has no
  trailing newline — but a source ending in `\n` produces a trailing empty
  segment, which contributes one. That asymmetry is upstream's, and is preserved.
- Leading blank lines within a unit are dropped (a consequence of the separator
  rule).
- A leading UTF-8 BOM is stripped; several cases have one, and it would otherwise
  hide the first directive.
- Non-comment content before the first `@filename` makes upstream **panic**. We
  record it on `TestCase::error` instead — a harness must not die on one malformed
  case out of 12,444. Currently zero cases trigger it.

Getting any of this wrong misattributes source to the wrong file, which corrupts
every downstream comparison in a way that looks like a compiler bug. That is why
`corpus_ingest` is a suite of its own: if it drops below 100%, fix the harness
before trusting any other number.

## Snapshot format

Modelled on oxc's `tasks/coverage/snapshots/*.snap`. Snapshots are **checked in**,
which is the point: a regression appears as a reviewable diff in the pull request
that caused it.

```
commit: 5b1047d10

parser_typescript Summary:
What a pass means: the source parses and produces the expected parse diagnostics
Passed         : 0/12444 (0.00%)
Unsupported    : 12444

  12444  no scanner or parser implemented yet (bd tsr-pum)
```

Every snapshot records the upstream commit it measured against; a percentage
without a pin is not reproducible. Failure detail is capped at 100 entries so one
regression stays readable, but the *count* is always reported in full — truncation
hides detail, never magnitude.

## Parallelism

Suites run with `rayon` over cases. Outcomes are collected in case order and
tallied afterwards, so snapshots are byte-identical regardless of scheduling —
verified at 1, 3, and default thread counts. The full run is ~25 s of CPU in
~4.6 s wall. See [threading.md](threading.md).

## Not yet built

- **Per-configuration runs** for the 793 varied cases.
- **Per-configuration runs** are still the largest excluded group (793 cases).
- **`.types` / `.symbols` / `.js` suites** for the binder, checker, and emitter.
- **fourslash**, needed for the language service (`bd` epic `tsr-5o3`).
