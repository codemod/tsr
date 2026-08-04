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
parser_reachable_target              5031/10570    47.60%      1874
scanner_termination                 12444/12444   100.00%         0
scanner_clean_files                   5031/5031   100.00%      7413
parser_typescript                     5000/5031    99.38%      7413
binder_symbols                        8284/8451    98.02%      3993
module_resolution                         95/95   100.00%     12349
file_loader                               96/96   100.00%     12348
```

The first two measure the **harness**; `parser_reachable_target` measures the
**size of the target**; the scanner suites, `parser_typescript`,
`binder_symbols`, `module_resolution` and `file_loader` measure the **compiler**.

The last two are deliberately two suites over one oracle: the 146 `.trace.json`
baselines have two authors, and measuring them together fuses two failure modes
into one number. `module_resolution` judges what a resolution *did*;
`file_loader` judges which resolutions were *asked for*, in what order, and in
what mode. Their denominators differ by exactly `+5 −4`, derived in
[module-resolution.md](module-resolution.md); anything else is a bug in a suite.
Everything before the judging is shared, in `src/trace_case.rs`, so they can only
drift where a suite says it is less capable.
See [ADR-0018](../adr/0018-splitting-the-resolution-oracle.md),
[ADR-0019](../adr/0019-the-loader-gate-discharges-the-mode-circularity.md) and
[ADR-0020](../adr/0020-the-parser-reads-tsconfig.md).

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

## The over-report ceiling on `diagnostics`

`examples/over_reports.rs` answers the one question the `diagnostics` suite cannot:
**which diagnostics do we emit that upstream does not?** That suite fails a case for
emitting too much or too little, and with no checker almost every case fails for too
little, so a false positive is invisible in its rate. It is also the one class of
failure the checker cannot fix — a parser or binder diagnostic upstream never emits
will still be there underneath the checker's output.

Measured 2026-08-05 at `06d30b3`:

| component | over-reported diagnostics | cases |
|---|---:|---:|
| parser / scanner | 7,234 | 509 |
| binder | 126 | 47 |

**502 of the 5,488 cases `diagnostics` judges carry one, so that suite is capped at
90.9% until they are fixed**, however good the checker is. Re-derive with
`cargo run --release -p tsr-conformance --example over_reports`.

The split is 57:1 against the parser, and its shape differs: the binder's 126 are
spread thinly (TS2300 99, TS2528 15, TS2451 12 — `bd tsr-y4u.19`), while the
parser's concentrate in error *recovery* — TS1012 (2,456 over 192 cases), TS1005
(1,907/324), and TS1127 (1,812 over just **8** cases, a per-character cascade).
`bd tsr-pum.11`. The tool prints a `shadowed` column — how many land where upstream
reports something else — which separates "we invented an error" from "we got the
code or the owner wrong".

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

## The binder's oracle: `.symbols` baselines

The submodule commits **12,482 `.symbols` files**. For every identifier occurrence
in a case, upstream records the symbol it resolved to and where every declaration
of that symbol is:

```text
=== ClassDeclaration14.ts ===
class C {
>C : Symbol(C, Decl(ClassDeclaration14.ts, 0, 0))

   foo();
>foo : Symbol(C.foo, Decl(ClassDeclaration14.ts, 0, 9))
```

This is a better oracle than a symbol-table dump, and the reason is worth
recording: a dump would compare our data model against upstream's, and the two are
deliberately different ([ADR-0003](../adr/0003-tree-plus-side-tables.md),
[ADR-0013](../adr/0013-checker-memoisation.md)). Resolution *results* are
model-independent, so the baseline judges the thing we care about without
constraining how we store it.

`binder_symbols` currently asks a subset: for each symbol upstream names, did we
create one of the same qualified name, declared on the same lines? That covers
symbol creation, scope placement, and declaration merging.

### Three things the format taught us, each of which was a bug in the harness

The first measurement read **23.59%**, and almost all of the gap was the harness
rather than the binder.

**Members are qualified.** The baseline writes a member as `C.foo`, `C[1]`, or
`C["bar"]` depending on how it was written, while we store the bare name and a
parent link. Worth +0.4% — much less than expected, which is what sent us looking
further.

**Not every symbol is ours to produce.** A reference to `console` resolves to
`Decl(lib.dom.d.ts, --, --)`. We do not load lib files, so requiring those symbols
scored the absence of a standard library as a binder failure. Excluding
declarations from other files removed 73 cases from the denominator.

**Positions are full starts, and that changes the line.** `Decl(…, 0, 9)` for
`foo` on line 1 is the position of the `{` that precedes it: TypeScript's
`node.pos` is where a node's *leading trivia* begins, not where its first token
does. Comparing token starts disagrees almost everywhere, and — the part that
matters — comparing token-start *lines* does not dodge it, because the previous
token is often on the line above.

That one was worth **+38 points**, 24.14% → 62.05%.

The lesson is the same one the parser's `allocs/op` diagnosis taught: a
measurement that disagrees with a trusted oracle is more likely to be measuring
the wrong thing than to have found a defect. Three rounds here, three harness
bugs, no binder bugs.

### What is approximate, and what is not covered

- **Full start is recovered, not recorded.** We do not store it on nodes, so the
  suite walks back over trivia from the token start. That is exact for whitespace
  and block comments and wrong for `//` comments, which cannot be recognised
  scanning backwards. Recording full start properly is filed.
- **Lines, not columns**, because of the above.
- **Resolution is not tested.** The baseline says which *occurrence* binds to which
  symbol; recovering an occurrence's position means reconstructing the
  interleaving of source and annotations. Filed.
- **Multi-file cases are compared unit by unit.** Each baseline section names a
  unit and each unit is bound alone, which is what the compiler currently is:
  there is no program and no cross-file linking, and the comparison already drops
  any symbol whose declarations live in another file. See
  [binder.md](binder.md#the-denominator-changed-on-purpose) for what that did to
  the denominator.
- **A symbol may be accepted under more than one name.** Upstream prints
  `checker.symbolToString(symbol, node.parent)` — the shortest name reachable
  *from the reference site* — and choosing between the spellings is a resolution
  we have no checker to redo. So the harness offers all of them and accepts a
  match on any:
  - every **dotted suffix** of the qualified name, because a use of `m` inside
    `namespace M { export class C { m() {} } }` prints `C.m` and a use from
    outside prints `M.C.m`;
  - for a **default export**, both `default` and the name its declaration was
    written with, because `getNameOfSymbolAsWritten`
    (`internal/checker/nodebuilderimpl.go:978`) prints the declaration's name for
    a reference in the same file and `default` for one from outside.
    `export default function foo` therefore matches either, while
    `export default class {}` has no written name and matches only `default`.
  - for a member written with a **computed name**, the source text of that name:
    `C[Symbol.iterator]`, `[foo()]`, `[-1]`. Upstream prints exactly this —
    `getNameOfSymbolAsWritten` falls through to the declaration's name node, and
    `declarationNameToString` of a `ComputedPropertyName` is the text it was
    written with — and it does so whether or not the name was ever resolved.

  Each of these weakens the test — a suffix could match a same-named symbol
  nested elsewhere — and that is the price of not having a checker yet.

### What accepting the written form of a computed name gives up

This one deserves its own note, because an earlier version of this document
argued the opposite: *"Computed names are deliberately left alone. Normalising
those would turn a real gap into a passing case."*

That was correct while the binder created **no symbol at all** for `[k]`.
Matching on the written spelling would then have scored a missing symbol as
present. It is no longer correct, because the binder now creates the symbol
upstream creates — `__computed`, parented to the container, carrying the
declaration — and matching on the spelling tests exactly the three things the
binder is responsible for: that a symbol exists for the declaration, that it is
in the right container, and that it is at the right line.

What it stops testing is **late binding**: that two spellings of the same
computed name are one symbol, and that `[k]` merges with a declared `a` when `k`
is `"a"`. Those are the checker's, and the suite still catches the merging half —
the baseline lists every declaration of a merged symbol, and the comparison
requires ours to contain them all. Four cases fail on precisely that, which is
the evidence the oracle still has teeth here.

The split between binder and harness was measured, not assumed: with this
accommodation in place and the binder's `__computed` symbol switched off, the
suite reads 92.88% — the rate before either change.
