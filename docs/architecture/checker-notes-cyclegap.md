# The gap's unattributed remainder — `cyclegap.rs`

Measured at `7299a14`, eighth session. Instrument: `crates/tsr-conformance/examples/cyclegap.rs`.
Gap walked: **80,315 lines**, identical to `depend.rs` at the same commit.

This page exists because `STATUS.md` §4.0 closed the seventh session with
**"no unowned row remains above 1,000"**, and that sentence is true only of the
rows `depend.rs` *attributes*. Two of its five endings attribute nothing, and
this probe measures what is behind them.

## 1. The forcing constraint

`depend.rs` buckets a gap line by **the kind of the node its walk stopped on**.
Three of its endings name a cause (`the dependency types — the root is here`,
`no further dependency`, `NO STEP ARM`). Two do not:

| ending | lines | what the kind column means there |
|---|---:|---|
| `cycle` | 5,394 | the walk re-entered a node; the kind printed is *an arbitrary member of a loop* |
| `depth cap` | 1,330 | the walk ran 16 steps; the kind printed is *wherever it happened to be* |

`depend.rs`'s own C2 documents the cycle ending as **"a real shape here
(`var a = b; var b = a;`)"**. That is a hypothesis about the population and it
had never been tested. Together the two endings are **6,724 lines, 8.4% of the
gap** — larger than every named board row below `NewExpression`.

A second constraint, independent of the first: a kind histogram cannot say what
the *answer was supposed to be*. The baseline's text is already parsed in order
to decide the line gaps, so the shape of the wanted type is free and had never
been reported.

## 2. What it measures

1. **The want shape** of every gap line — the syntactic form of the type
   upstream prints, classified **outside-in** at bracket depth 0 (a union of
   signatures is a union; `(() => void)[]` is an array, not a signature — the
   missing mechanism is the outer constructor, not the inner one).
2. **The kind sequence of the loop itself** for the cycle ending, so a genuine
   recursive shape can be told from a probe artefact.
3. For a length-1 cycle, **what the node declares** and whether it carries an
   annotation or an initialiser.
4. For a `NO STEP ARM` **type-node** root, whether a child type node also gaps —
   i.e. whether an arm would have had anywhere to step.

Controls: **C1** arithmetic (endings and shapes each sum to 80,315 — both do);
**C2** frozen (cycles and depth-cap hits must reproduce `depend.rs`'s 5,394 and
1,330 — both do exactly, which is what makes the copied walk readable);
**C3** `want-any` reported per row, because ADR-0038 puts those beyond reach
whatever the shape says.

## 3. The finding: the `cycle` ending is not a cycle

```
  5,319 of 5,394 cycles have loop length ONE — the node's step is ITSELF
```

Split by what the node declares, every one of them has **neither an annotation
nor an initialiser**:

| lines | declaration | annotation | initialiser |
|---:|---|---|---|
| 2,943 | `FunctionDeclaration` | no | no |
| 2,359 | `BindingElement` | no | no |
| 17 | `ModuleDeclaration` | no | no |

That is `step`'s declaration-name arm falling through — no annotation to take,
no initialiser to take — into its *identifier-reference* arm, which resolves the
name to its symbol, takes the symbol's value declaration, and returns that
declaration's name: **the node it started from**.

**So both rows are `NO STEP ARM` wearing the `cycle` label.** `depend.rs` added
the `NO STEP ARM` ending precisely so that *"this probe cannot walk out of this
kind"* would stop being reported as a finding about the compiler — and it applies
that relabelling only to the `no further dependency` ending (`depend.rs:395`).
A kind that self-loops instead of returning `None` slips past it.

The genuinely recursive shape its C2 describes is **75 lines**, not 5,394:
56 `Identifier -> TypeReference`, 10 `Identifier -> Identifier`, 9 longer.

**What the two rows actually are**, from the want column:

- **`FunctionDeclaration`, 2,943 lines, want-any 1.8% (net 2,889), 611 cases,
  top-1 3.7%.** 1,894 want a **signature / arrow**, 413 a primitive, 213 a union.
  A function declaration with no return annotation, whose own name gaps: the
  wanted answer is the function's signature text. This is the lowest want-any
  share and the most diffuse case distribution of any row above 2,000 on the
  board.
- **`BindingElement`, 2,369 lines, want-any 32.5% (net 1,598), 256 cases.**
  A binding element with neither annotation nor initialiser — its type comes
  from the pattern's source, which is `bd tsr-84iz`/`tsr-pqnh` territory.

The `depth cap` ending needs no such correction and is **not** an item: **all
1,330 of its lines are two pathological cases** —
`parsingDeepParenthensizedExpression` 1,051 (`BinaryExpression` 738,
`ParenthesizedExpression` 293, `ElementAccessExpression` 20) and
`longObjectInstantiationChain2` 279 (`CallExpression` 112, and three access /
identifier rows) — every one of its seven rows reading top-1 100.0%. The two
sums are `1,051 + 279 = 1,330` exactly, so the ending has no diffuse remainder
at all.

## 4. The second finding: 4,696 type-node "roots" are propagation

`step` has arms for expressions, identifiers and `TypeReferenceNode`. Every
other type-node kind terminates as `NO STEP ARM`. Asking whether a **child type
node also gaps** says whether an arm would have had somewhere to go:

```
  8,499 type-node roots with no arm
  4,696 PROPAGATING — a child type node gaps; the root is inward
  3,803 terminal — this node's own arm is what refused
```

| lines | kind | a child type node gaps? |
|---:|---|---|
| 2,114 | `ArrayType` | **yes** |
| 806 | `TypeLiteral` | no |
| 777 | `UnionType` | no |
| 713 | `TupleType` | **yes** |
| 475 | `TypeOperator` | no |
| 428 | `UnionType` | **yes** |
| 403 | `TypeQuery` | no |
| 339 | `IntersectionType` | **yes** |

**`ArrayType` — the board's 10th row at 2,120 lines — is 99.7% propagated.**
`STATUS.md` §4.0 carries it as `ArrayType 2,120 (94.3% one case)`; the one-case
concentration is real and the *row* is not a root at all. Its 2,087 array-shaped
wants are `T[]` prints whose element type is what fails.

This does not create work: it moves ~4,700 lines off the board's root columns
and onto whatever is inside them, and the honest statement is that **the board
under-reports the concentration of the gap in a smaller number of causes**.

## 5. The gap by what upstream's answer looks like

New, and the only whole-gap view that is not a node-kind histogram:

| shape | lines | share |
|---|---:|---:|
| `any` (ADR-0038 ceiling) | 19,970 | 24.9% |
| primitive | 14,652 | 18.2% |
| **signature / arrow** | **12,606** | **15.7%** |
| bare name | 6,411 | 8.0% |
| anonymous object | 5,799 | 7.2% |
| array | 5,152 | 6.4% |
| generic reference | 4,177 | 5.2% |
| union | 4,112 | 5.1% |
| typeof query | 2,109 | 2.6% |
| qualified name | 1,425 | 1.8% |
| tuple | 1,414 | 1.8% |
| string literal | 1,406 | 1.8% |
| intersection, numeric literal, `import(…)` | 1,082 | 1.3% |

**A quarter of the remaining gap wants `any`** — the largest single fact on the
page, and the one that should be quoted whenever the distance to 80% is
discussed. §2's *firm* ceiling of 2,202 is an attribution count and is not this
number; these 19,970 lines are lines this port currently fails to compute *and*
whose baseline answer is `any`, which is exactly the unstable population §2 warns
about. Neither figure supersedes the other and they must not be substituted for
one another.

**"Signature / arrow" at 12,606 lines is the largest shape that is not a ceiling
or a primitive, and it does not correspond to any single board row.** It is
spread across `ArrowFunction` 3,328, the mislabelled `FunctionDeclaration` cycle
1,894, `PropertyAccessExpression` 908, `FunctionExpression` 832,
`FunctionDeclaration / dependency` 664, and a long tail. This is §4.4's
*"structured signature types"* capability measured from the answer side for the
first time, and it is the largest thing on this page.

## 6. What this does and does not license

- It does **not** produce a build. Nothing here is scored, and quoting any of
  these populations as work would break `STATUS.md`'s fourth rule.
- The `FunctionDeclaration` self-loop row (2,889 net, 1.8% want-any, 611 cases)
  is the strongest *unowned* candidate the board has had since the seventh
  session closed, and the next step on it is a counterfactual — does printing an
  un-annotated function declaration's inferred signature match the baseline
  text — not an arm.
- Two fixes belong in `depend.rs` and are deliberately **not** made here, so the
  board's published numbers stay comparable across sessions: relabel a length-1
  cycle as `NO STEP ARM`, and give `step` arms for `ArrayType`, `TupleType`,
  `UnionType`, `IntersectionType`. Both change the board's rows and neither
  changes the compiler.

## 7. The row split — `fnsiggap.rs`, same session

§3 named the `FunctionDeclaration` self-loop row as the strongest unowned
candidate and said the next step was a split, not an arm. Taken immediately,
because a row is not a mechanism:

```
population 2,943   (C1 reproduces cyclegap.rs exactly)

  1,016  34.5%  downstream: a parameter gaps                    net 1,010
    985  33.5%  downstream: a returned expression gaps          net   969
    605  20.6%  WIRING: no return statement at all              net   574
    205   7.0%  WIRING: every parameter and every return types  net   205
    132   4.5%  downstream: a parameter AND a return gap         net   131
```

**72.5% of the row is downstream and is not an item.** It belongs to its inputs,
which is `retgap.rs`'s conclusion about the *other* `FunctionDeclaration` row
reproduced on this one (`checker-notes-callres.md` §12). The buckets are ordered
downstream-first by construction (C2), so this split **under**-reports the cheap
answer — the safe direction.

**What is left is 810 lines, 779 net**, and it is four mechanisms, not one:

| lines | form | the inferred return is |
|---:|---|---|
| 366 | plain | the body's type |
| 228 | `async` | `Promise<T>` |
| 173 | generator | `Generator<…>` |
| 43 | `async` generator | `AsyncGenerator<…>` |

548 of the 810 take **no parameters at all**, which caps how much of the build
is parameter handling.

The row also splits **1,475 own-node / 1,468 reached after ≥1 step**, so half of
it converts only through the cascade — a caveat that travels with any forecast,
in both directions: those lines cannot be claimed by an arm that fixes only the
own-node case, and they may also be reached by mechanisms other than this one.

**This is a real but small item: ~779 net, ≈0.16 gradient points at 100%
conversion, and no build has ever converted 100%.** Recording it at that size is
the point — §3 called it "the strongest unowned candidate", and the split shows
that the strongest unowned candidate on this board is worth under two tenths of
a point. That is §4.4's conclusion arriving through yet another door.

## 8. How I would know I was wrong

- **C2 is the falsifier for the whole page.** The walk is copied from
  `depend.rs`; if the cycle and depth-cap counts ever stop reproducing it
  exactly, the copy has drifted and nothing above is readable.
- The self-loop diagnosis is falsified by a single length-1 cycle whose
  declaration *has* an annotation or an initialiser. Measured: zero, across all
  5,319.
- The want-shape classifier is falsified by any row whose top shape contradicts
  its known owner. Two independent checks pass: `ObjectLiteralExpression`'s top
  shape is *anonymous object* (1,926 of 2,383) and `TemplateExpression`'s is
  *primitive*/`string literal` (1,220 + 634 of 1,890) — both the shapes those
  rows' owners predict.
