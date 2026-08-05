# Which rule prints `any`, and what that does to the gradient

Status: measured at `9bbb36f`, in a detached worktree with its own submodule
checkout and its own `CARGO_TARGET_DIR`, one pinned binary per figure.
`9bbb36f` was `origin/main` when the runs were taken; the tip has since moved to
`014208c` and **none of these numbers were re-taken there** — `checker-notes-guard.md`
records a pair that appeared to move by 207 lines purely because teammates' edits
landed between two `cargo run`s, which is why the commit is named rather than
"the tip". `bd tsr-7xs` (the split) and `bd tsr-qj4` (the instrument).

Instrument: `crates/tsr-conformance/examples/any_audit.rs`, new here.

## 0. The findings, in the order they change decisions

1. **No arm in this checker mints `any` as a shrug.** All twelve sites that can
   return `intrinsics.any` are upstream-anchored rules; the four remaining
   `any`-printing rules live in the *producer* and port upstream's baseline
   **writer**. The rule ADR-0038 and ADR-0039 turn on is held on the minting
   side. §2 enumerates the sites; §3 measures where the lines land.
2. **The banked 21,685 is not materially inflated. The upper bound on defaulted
   `any` is 1,041 lines — 4.8% of it, 0.22 gradient points**, not the ~4.5 points
   the exposure was feared to be. §4.
3. **But a third of it is not a checker answer at all.** 7,291 banked lines
   (33.6%) come from a writer rule where *neither* compiler computed a type.
   They are faithful and they are legitimately scored, and anybody reading
   "21,685 lines we answered `any`" as "21,685 lines this checker computed" is
   wrong by a third. §3.
4. **`examples/wrong_attribution.rs` was measuring a different compiler**, the
   fourth instance of `docs/conventions.md`'s harness rule. Corrected, its gap
   column falls **173,537 → 139,612** and its wrong column rises **22,067 →
   37,489** — a third of what it called a gap was a lib file it never loaded.
   §5.
5. **`checker-notes-ctx.md`'s OPEN falsifier is half-settled, and the half that
   settles is the half that matters.** Of the 989 wrong lines
   `crates/tsr-checker/src/members.rs`'s `any`-receiver arm prints today, **805
   (81.4%) originate in an unannotated parameter in a container upstream can
   contextually type** — exactly the ownership `members.rs` asserts. The other
   half of the falsifier — "did the population drop by 548" — is **not
   settleable**, and §6 says why rather than guessing.

---

## 1. The instrument

`examples/any_audit.rs` runs over the same 9,538-case population the gradient is
scored over, through `types_producer::assertions_for_case_with_ids` — one
program per case with every bundled lib in it. For each aligned assertion line
whose type string is `any` it re-derives **which rule printed it**, by mirroring
`types_producer::type_at_location`'s branch order and taking the first branch
that returns.

### The denominator is the gradient's by construction

```text
upstream lines: 478954
aligned lines:  468900
exactly right:  291799  (62.23% of aligned)
```

`cargo run -p tsr-conformance --bin coverage` at the same tip reports
**291,799 / 478,954 = 60.92%**. Both numbers are re-derived by this probe from
the same entry point, so there is nothing to reconcile: the numerator and the
denominator are identical, and the 468,900 aligned figure is the subset on which
a *type* can be compared at all (`types_shapes.rs` explains the alignment test).

That reconciliation is the check `docs/conventions.md` demands before any of the
rows below can be quoted, and it is the check that found `writer_guards` and
`wrong_attribution`.

### Three controls, all zero, over 28,973 samples

```text
CONTROLS (must read zero):
  UNCLASSIFIED, banked      0
  UNCLASSIFIED, lost        0
  DISAGREEMENT (both)       0
```

- `UNCLASSIFIED` — a printed `any` reached by no branch of the mirror.
- `DISAGREEMENT` — a branch the mirror took whose type is **not** `any`, which
  would mean the mirror and the producer had drifted.

They are asserted, not merely printed: the probe panics if their sum is
non-zero. A classifier that has drifted produces a tidy table either way, and
`types_producer::gap_reason`'s own history — an earlier version invented a
22,768-line finding by not following the code it explained — is why this is an
assertion rather than a footnote.

### One row exists only because the control was chased to zero

The first run read `UNCLASSIFIED = 7`. They are not `any` at all: they are
declarations **named** `any` — `class any {}` in
`compiler/primitiveTypeAsClassName`, `compiler/enumWithPrimitiveName`,
`compiler/ClassDeclaration24` — whose printed name is the same string. They have
their own row (`NAMED-any`, 7 lines) rather than being folded into either
answer, and the same collision is present in `rank_board`'s own `any` audit,
which keys on the printed string and therefore counts them.

Seven lines change nothing. The point is that the control found them.

---

## 2. Every site that can answer `any`, classified by reading it

`grep -n 'intrinsics\.any' crates/tsr-checker/src` returns 16 hits at `9bbb36f`,
four of which are prose in comments. The twelve live sites, each verified
against `vendor/typescript-go` at `5b1047d10` with `grep -n`/`sed -n`:

| site | rule | upstream | computed? |
|---|---|---|---|
| `declared.rs:27` | the `any` keyword in a type node | `checker.go:22811` | yes, trivially |
| `symbols.rs:780` | a variable-like declaration with **no annotation and no initialiser** | `checker.go:18264` | yes — the implicit `any` |
| `symbols.rs:767` | a self-referential **initialiser** (an annotation gives `errorType`) | `checker.go:18822` | yes |
| `symbols.rs:563` | a shorthand ambient module — `declare module "x";` | `checker/utilities.go:198` | yes |
| `symbols.rs:271` | an accessor with no annotation and no getter body | `checker.go:18545` | yes |
| `signatures.rs:317` | a signature with **no body** — ambient, interface method, overload | `checker.go:20016` | yes |
| `members.rs:99` | a property access on an `any` receiver | `checker.go:11314`/`:11318` | yes |
| `indexed.rs:107` | an element access on an `any` receiver | `checker.go:11266` | yes |
| `binary.rs:220` | `+` with an `any` operand | `checker.go` addition rules | yes |
| `unions.rs:358` | a union containing `any` (`IncludesError` wins) | `checker.go:25659` | yes |
| `intersections.rs:131` | an intersection containing `any` (same) | `checker.go:26092` | yes |
| `expressions.rs:676` | `yield` outside, or in a non-contextualisable, generator | `checker.go:11005` | yes |

**There is no thirteenth site, and that is the finding.** Every one answers `any`
because upstream answers `any` for the same node; none answers it because the
port could not do better. Three of them go further and defend the distinction:
`members.rs:99` and `indexed.rs:107` test **identity** against `intrinsics.any`
rather than `TypeFlags::ANY`, because `errorType` also carries `ANY` and a flag
test would convert every gap in the corpus into a confident `any`;
`symbols.rs:271` explicitly routes its `None` to `errorType` so that an
inference gap cannot fall into the `any` below it.

### The one place the discipline could still leak, and where it goes

`symbols.rs:780` answers `any` when
`get_type_for_variable_like_declaration` returns `None`. Read from the outside,
that is exactly the shape of a defaulted answer — "we could not compute one, so
`any`". It is not, and the reason is the dispatch above it:
`get_type_of_variable_or_parameter_or_property_worker` routes **only**
`VariableDeclaration | Parameter | PropertyDeclaration | PropertySignature` to
it (`symbols.rs:841`), and for exactly those four kinds
`type_annotation_of`/`initializer_of` are total. So `None` means "no annotation,
no initialiser, and no contextual parameter type" — upstream's own
`checker.go:18264`. Every other declaration kind reaches `errorType` through the
`_` arm, not `any`.

The residual leak is therefore **one shape only**: an unannotated *parameter*
whose contextual type upstream computes and `crate::contextual` does not. That
is measured in §4 and it is the entire exposure.

### The four rules that are not in the checker at all

`types_producer::type_at_location` prints `any` at four positions where **this
checker holds `errorType` and so does upstream's** — they are ports of upstream's
baseline *writer* (`type_symbol_baseline.go:380`, `:481`), not type answers:
the left of a qualified name in type position, the property name of a binding
element, a label name, and an intrinsic JSX tag name. `checker-notes-jsx.md`
makes the same point for the JSX rule specifically. Counting these as evidence
about the checker's discipline — in either direction — is a category error, so
the probe separates them before anything else.

---

## 3. The split

**The question the rows answer**, stated before the sum as
`docs/conventions.md` requires: *which rule minted the printed `any`*. Not "what
would upstream have said here", which nothing on this side of the port can
answer, and not "where did the `any` that flowed into this rule come from",
which the `<-` suffixes name for the access rows and nothing else.

```text
BANKED — we print `any` and upstream printed `any`: 21685
  CHECKER          14387  (66.3%)
  WRITER            7291  (33.6%)
  NAMED-any            7  (0.0%)
```

The banked total is **21,685 — the same figure `rank_board` reported at
`33e3bd5`**, and the lost total is **7,288**, also unchanged. Two probes written
independently, three tips apart, agreeing to the line on both columns.

| row | lines | cases | top-1 | top-10 |
|---|---:|---:|---:|---:|
| WRITER / qualified-name left in type position | 5,275 | 347 | 69.9% | 82.0% |
| CHECKER / annotation denotes `any` (declaration name) | 3,616 | 1,031 | 6.6% | 25.1% |
| CHECKER / annotation denotes `any` (reference) | 2,341 | 360 | 6.0% | 31.1% |
| CHECKER / implicit `any`: parameter, container **cannot** be contextually typed | 1,303 | 337 | 8.2% | 32.2% |
| WRITER / intrinsic JSX tag name | 994 | 199 | 3.5% | 26.3% |
| CHECKER / implicit `any`: `VariableDeclaration`, no annotation, no initialiser | 935 | 354 | 3.4% | 19.6% |
| CHECKER / the same, at a reference | 815 | 61 | **60.9%** | 81.2% |
| WRITER / label name | 594 | 93 | 10.1% | 42.1% |
| CHECKER / implicit `any`: parameter, container **CONTEXTUALISABLE** | 544 | 216 | 9.4% | 27.6% |
| CHECKER / implicit `any`: `PropertyDeclaration` | 435 | 165 | 14.5% | 35.4% |
| WRITER / binding element: the property name | 428 | 92 | 6.8% | 40.9% |
| CHECKER / implicit `any`: `PropertySignature` | 383 | 120 | 32.9% | 52.5% |
| CHECKER / `AsExpression` | 328 | 114 | 17.1% | 52.1% |
| CHECKER / implicit `any`: parameter, not contextualisable, at a reference | 313 | 71 | 23.0% | 65.2% |
| CHECKER / `CallExpression` | 312 | 105 | 6.4% | 43.3% |
| CHECKER / implicit `any`: parameter, CONTEXTUALISABLE, at a reference | 275 | 119 | 9.8% | 34.9% |
| CHECKER / property access on an `any` receiver ← an `: any` annotation | 258 ×2 | 58 | 17.4% | 62.4% |
| CHECKER / `yield` in a non-contextualisable container | 210 | 97 | 10.0% | 45.7% |

The two largest CHECKER rows — 5,957 lines between them — are lines where the
source **says `: any`**. There is nothing to audit there: the programmer wrote
the type. Adding the four writer rows (7,291) and those two (5,957) accounts for
61% of the banked column with rules that cannot be defaulted even in principle.

Concentration is worth reading on two rows. The qualified-name-left row is
**69.9% one case**, so it is one file's worth of a rendering rule rather than a
corpus-wide phenomenon; and the 815-line implicit-`any` reference row is 60.9%
one case. Neither changes the verdict, but neither is the distributed population
its size suggests.

---

## 4. The exposure, and it is 4.8% rather than 100%

```text
EXPOSURE — the share of the banked 21685 that is not a type this checker
computed from an upstream-anchored rule on inputs upstream would have had:
  writer rules (neither compiler computed a type)            7291  (33.6%)
  unannotated parameter in a contextualisable container      1041  (4.8%)
```

**1,041 lines is the upper bound on defaulted `any` in the banked column** —
0.22 gradient points, against the ~4.5 points the whole 21,685 represents.

It is an upper bound in three separate ways, each of which would only shrink it:

1. The fence is `isContextSensitiveFunctionOrObjectLiteralMethod`
   (`checker.go:29496`) with its **second conjunct dropped** — this probe asks
   only whether the container is a function expression, an arrow or an
   object-literal method, and not whether the declaration is context-sensitive
   (`checker.go:30931`). A superset by construction.
2. Being in a contextualisable container does not mean upstream *had* a
   contextual type — a bare `const f = x => x` has none, and upstream's answer
   there is the implicit `any` too.
3. Where upstream's contextual type is itself `any` — a callback declared
   `(cb: (x: any) => void)` — both compilers compute `any` and the credit is
   real.

**The mechanism is not hypothetical, and the evidence that it is small is that
the same mechanism dominates the *lost* column.** Of the 7,288 lines we answer
`any` and upstream does not, **3,251 (44.6%) are an unannotated parameter in a
contextualisable container** and a further 805 are property accesses whose
receiver is one. When our contextual gap meets upstream's real answer, the line
lands in *lost*; it lands in *banked* only when upstream's answer happened to be
`any` as well. 1,041 banked against ~4,000 lost is the shape that predicts.

### What would falsify this

Two ways, both cheap:

- **A thirteenth site.** Re-run `grep -n 'intrinsics\.any' crates/tsr-checker/src`
  and read any hit this table does not list. One arm answering `any` on a path
  whose precondition is "we could not compute a type" falsifies §2 outright, and
  the enumeration is sixteen lines of grep.
- **When contextual typing closes.** If the CONTEXTUALISABLE parameter rows
  shrink and the banked total falls with them, those lines were defaulted and
  this section under-counted. If the rows shrink and the banked total holds —
  because upstream's contextual answer was `any` anyway — they were computed.
  The probe prints both, so the next contextual slice settles it as a side
  effect rather than as an errand.

### What this does NOT license

Nothing here weakens `docs/conventions.md`'s rule or ADR-0038's refusal. The
7,288 lost lines are a *confirmed* violation of the spirit of it — a confident
`any` where upstream has a real type — and the discipline is what keeps the
number at 7,288 rather than at 139,612. **No row should be closed by widening an
`any` answer.** What is now measured is that nobody has been doing so.

---

## 5. `wrong_attribution` was measuring a different compiler

`bd tsr-qj4`, fourth instance, third file. `crates/tsr-conformance/examples/wrong_attribution.rs`
built one `tsr_checker::Checker` per **file** — `parse_with_options` → `bind` →
`Checker::new`, no `Program`, no bundled libs. Corrected to
`types_producer::assertions_for_case_with_ids`. Both sides run at `9bbb36f`, one
pinned binary each, nothing else in the tree changed between them:

| | lib-less (wrong) | with libs (correct) |
|---|---:|---:|
| aligned lines | 468,921 | 468,900 |
| gaps (we said `error`) | 173,537 (37.01%) | **139,612 (29.77%)** |
| wrong (a claim) | 22,067 (4.71%) | **37,489 (8.00%)** |
| of which `Identifier` | 17,828 | 31,067 |
| lib arm, positive control | 4,224 | **397** |
| `typeof {upstream}` defect | 1,086 | **88** |
| implicit-`any` population | 6,444 | 6,418 |

Three things to take from it.

**The corrected `wrong` total is 37,489, which is exactly what
`examples/rank_board.rs` reports** over the same population by an independently
written path. Two instruments agreeing to the line is the evidence that the
correction landed; before it they disagreed by 15,422.

**The direction is the one that matters.** 33,925 lines moved out of the *gap*
column into the *wrong* column. A lib-less checker does not merely under-perform:
it converts confident wrong answers into honest-looking gaps, which is the
failure mode that makes a probe comfortable to read.

**The published caveat was false, and it was load-bearing.** The module doc
asserted that "the `checker_types` producer binds each unit ON ITS OWN and loads
no lib files", and concluded that **lib loading cannot move `checker_types` at
all**. That is the same false caveat `checker-notes-calls.md` found in
`overload_funnel`: it reads `binder_suite.rs`, which deliberately has no libs, as
though it were `types_producer.rs`, which has them. It is now marked FALSE in
place rather than deleted, per `docs/conventions.md`.

The `typeof {upstream}` row is worth naming separately because it was quoted as
a work item: **"1,086 of them are one defect" is 88.**

### `qualified_name_left.rs`, same defect, same fix

| arm | control | right | wrong | gap |
|---|---:|---:|---:|---:|
| left of qualified name, under `typeof` | 186 | 79 | 2 | 105 |
| left of qualified name, under a `TypeReference` | 5,266 | 5,266 | 0 | 0 |
| left of qualified name, other grandparent | 173 | 132 | 32 | 9 |
| right of qualified name (the type name itself) | 508 | 3 | 42 | 463 |
| UNATTRIBUTED (control) | 0 | | | |

**The verdict holds; the magnitude halves.** The item is **76** wrong lines, not
174, and the 4,455-line — now 5,266-line — `TypeReference` arm is still
zero-wrong. The 133-line import-equals rule reads 41, but `cb173ea` landed
between the two measurements, so this pair does not separate the instrument from
the fix and no share of the fall is attributable here.

### `types_walker.rs` is flagged, not fixed

It is on the same per-unit path but it stubs the type (`|_| String::new()`) and
measures node selection and text extraction alone, so the checker configuration
cannot reach its numbers. Its walker-agreement figure is independently
reconcilable: this probe's aligned/upstream ratio through the *case* path is
468,900 / 478,954 = **97.90%**, which is the same quantity. Fixing it is
bookkeeping; `bd tsr-qj4` should stay open for it rather than be closed by this
document.

---

## 6. `checker-notes-ctx.md`'s OPEN falsifier: half settled, and the half that settles is the useful one

> If the 548 reachable count does not correspond to a visible improvement in the
> property-access **wrong** population, then the 555 wrong lines in
> `crates/tsr-checker/src/members.rs` are not the shape that document claims.

### The half that is now measured

`members.rs`'s `any`-receiver arm accounts for **989 lost lines** today — 495 on
the access node and 494 on the member name, because the walker emits a line for
each. Split by the origin of the receiver's `any`, walking down the access chain
to the node that minted it:

| the receiver's `any` came from | lines | share |
|---|---:|---:|
| an unannotated parameter in a **contextualisable** container | 805 | 81.4% |
| an unannotated parameter that **cannot** be contextually typed | 85 | 8.6% |
| a variable with no annotation and no initialiser | 66 | 6.7% |
| an explicit `: any` annotation in the source | 28 | 2.8% |

**The ownership claim in `members.rs` holds at 81.4%**, with the same zero
control as every other row here. Those lines are contextual typing's, not the
property-access arm's, and the document's `const y = x => x.foo` probe describes
the population correctly.

Two corrections fall out of it, and both belong to the owner of
`crates/tsr-checker/src/members.rs` rather than here:

- **28 lines are not contextual typing's** — the receiver is annotated `: any`
  in the source, so upstream has the same receiver and a different answer. Small,
  but the comment says "every one of those 555 is the same shape", and it is not.
- **The count is 989 or 495, not 555**, depending on whether the member-name
  line is counted, and the comment does not say which it counted. Worth pinning
  when that comment is next touched.

### The half that is not settleable, and why saying so is the answer

The falsifier asks whether the population **dropped by ~548**. It cannot be
answered:

- **Its left-hand side is unreproducible.** `checker-notes-ctx.md` records this
  itself: the 548 comes from an instrument that counts 1,557 where the cycle-9
  note records 925, the cycle-9 counter was not preserved, and the disagreement
  is unresolved. A falsifier whose threshold cannot be re-derived is not a test.
- **Its right-hand side was never recorded as a population.** The 555 in
  `members.rs` is a **delta** measured when that arm landed — "+555 wrong lines
  against 894 gaps closed" — on the pre-`contextual.rs` tip and, given the dates,
  on the lib-less instrument. There is no before-population to compare 989
  against.

So: 555-as-a-delta against 495-as-a-population is not a subtraction anyone should
do, and this document does not do it. What can be said is that the arm's wrong
population today is 989 lines, 81.4% of which are the claimed shape, and that
**contextual typing as it stands has not removed them** — `crate::contextual`
answers some contexts and 805 lines remain whose parameters it does not type.
Anyone reading the ctx note as saying the 555 are gone should read that instead.

**The falsifier should be restated in terms this probe can re-derive**, and that
is a change to `checker-notes-ctx.md` for its owner to make:

> When the next contextual arm lands, the row
> `property access on an `any` receiver ← implicit `any`: unannotated Parameter,
> container CONTEXTUALISABLE` in `examples/any_audit.rs` must fall by at least
> the number of parameters that arm types. If it does not, the lines are not
> contextual typing's.

That version has a reproducible left-hand side, a reproducible right-hand side,
and it is one bucket of a probe that already runs.

---

## 7. What this cost, and the trap it nearly walked into

The first design of this audit was going to add a counter to each
`intrinsics.any` site inside `tsr-checker` — the shape `calls.rs`'s
`classify_unresolved_callee` uses, and the shape the assignment suggested. It
would have been the wrong instrument for the question. A per-site counter counts
**invocations**, and the question is about **printed lines**; a single assertion
line can pass through the union rule, the property-access rule and the implicit
`any` on one path, and a counter cannot say which of them the baseline saw.

Attributing at the *printed line*, by mirroring the producer's dispatch, answers
the question directly and needs no checker edit at all. It also produced the
`NAMED-any` row and the writer/checker split, neither of which a site counter
could have seen.

The generalisable form, and it is a variant of `docs/conventions.md`'s
"pre-register on the most direct bucket your instrument produces": **an
instrument sited where the code is convenient measures the code; site it where
the answer is observed, and it measures the answer.**
