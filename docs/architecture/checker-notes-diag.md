# Checker notes — diagnostics, and what `TS2322` is actually worth

Working notes for `bd tsr-6re` / `bd tsr-8yu`, measured 2026-08-05 against
`vendor/typescript-go` @ `5b1047d10`. Every upstream anchor below was
`grep -n`-verified at that commit.

**The result in one line: no emitter was built, and the numbers say not to build
one yet.** `docs/adr/0040`'s sizing falsifier resolves against its own bounded
emitter, and its message falsifier resolves against its own reporting twin. Both
outcomes are recorded in ADR-0040 itself, dated.

The instrument is `crates/tsr-conformance/examples/assignability_shape.rs`:

```
cargo run -q --release -p tsr-conformance --example assignability_shape
```

---

## 0. The denominator was wrong, and it is the reason for every other correction

`bd tsr-6re`, `bd tsr-8yu` and ADR-0040 all quote a **7,025** population of
"diagnostic-bearing baselines", **516** `TS2322`-only cases, and **+536** cases
for `TS2322`.

There are **7,027** `*.errors.txt` files under
`vendor/typescript-go/testdata/baselines/reference/submodule`, and **1,180** of
them are configuration variants — `case(target=es5).errors.txt` and friends. The
`diagnostics` suite excludes every one of those by construction
(`CaseEntry::has_varied_errors`, `crates/tsr-conformance/src/corpus.rs:163`),
because there is no single expected output to compare a default compilation
against.

So the 7,025 is a **file** count over a population the suite does not judge. Over
the suite's own 5,488:

| | briefed (7,025 files) | measured (5,488 judged cases) |
|---|---:|---:|
| scanner + parser + binder only | 1,063 / 15.13% | **765 / 13.94%** |
| + `TS2322` | 1,599 / 22.76% | **1,254 / 22.85%** |
| + assignability family | 1,808 / 25.74% | **1,445 / 26.33%** |
| `TS2322` gains | **+536** | **+489** |
| cases whose only code is `TS2322` | **516** | **478** |

The *shares* survive almost exactly, which is why nobody caught it: 22.76% vs
22.85%. The *counts* do not. This is `docs/conventions.md`'s "a number can be true
and answer a different question", arriving through a denominator rather than a
sum — and the rule that catches it is the one already written there: **a probe's
denominator must be the gradient's by construction, not by resemblance.** The
probe prints both, side by side, so the two can never again be quoted for each
other.

`+489` is still the largest single reachable item on the board. Nothing below
disputes that. What is disputed is that a *bounded* emitter can collect it.

---

## 1. Falsifier 1 — sizing. **89 as an upper bound, 15 for the named slice.**

The question ADR-0040 says must be measured and must not be guessed: of the
`TS2322`-only cases, how many are primitive-to-primitive — inside `SELECTABLE`
(`crates/tsr-checker/src/calls.rs:75`), where a `false` from
`Checker::is_type_assignable_to` (`crates/tsr-checker/src/relater.rs:151`) is
trustworthy — rather than object-to-object, where it is a *confident wrong
diagnostic*?

Of the **478** judged `TS2322`-only cases:

| shape | cases |
|---|---:|
| **every** `TS2322` primitive → primitive | **89** |
| some primitive, some not | 25 |
| **none** primitive | **364** (76.2%) |

"Every" is the right quantifier and not a conservative flourish: the suite
compares the **exact multiset** of `(file, line, column, code)`, so a case
converts only when every diagnostic it expects is produced. A case with one
primitive `TS2322` and one object-to-object `TS2322` converts nothing.

**Three-quarters of the `TS2322`-only population is object-to-object.**
ADR-0040 stated the consequence in advance: *"If most of the 516 are
object-to-object, the real item is the complete relater and this ADR's bounded
emitter converts almost nothing."* That is the measured answer.

### 89 is an upper bound, and a loose one

The probe reads **upstream's `.errors.txt` only**. The type names it classifies
are the names upstream printed. It therefore assumes our port computes the same
type at the same position, which is unmeasured. 89 is the ceiling, not a
forecast.

### The number that decides slice one is 15

ADR-0040 names exactly one call site — `checkVariableLikeDeclaration`
(`checker.go:5790`), reporting at `checker.go:5899`. Of the 89, the cases whose
**every** `TS2322` sits on a `VariableDeclaration` name number **15**. That is
0.27% of the judged suite, against a build comprising a `check_source_file`
traversal, `checkExpressionCached`,
`getWidenedTypeForVariableLikeDeclaration`, a diagnostics collection on the
`Checker`, and the `SELECTABLE` bound — and 15 is itself an upper bound.

### Pre-registration, and what it said

Stated before the probe was written, in the form `docs/conventions.md` demands
(on the bucket the instrument prints, not on a proxy):

> `PRIMITIVE_ONLY_CASES < 50` → decline. `≥ 150` → build. Between 50 and 150 →
> build only if the position bucket is also concentrated in one syntactic form.

89 landed in the middle band. The positions are the **opposite** of concentrated
— see §3 — so the rule resolves to **decline**. And the `VariableDeclaration`
bucket, which is the level-4 "what does slice one *finish*" question, reads 15
and was not what the rule was written against. Recording both is the point:
`docs/conventions.md`'s newest entry is about a rule written one inference away
from the answer, and this is the same shape avoided by printing the direct bucket
beside the pre-registered one.

---

## 2. Falsifier 2 — message form. **Resolved, and it resolves *against* ADR-0040's
reporting twin.**

ADR-0040 decision (3) is that `is_type_assignable_to` gets a reporting twin taking
an error node, because "the message is an output of the walk". Its own falsifier:
*"if, restricted to the `SELECTABLE` domain, upstream's switch provably always
falls through to the generic message and never builds a chain, the consumer shape
is adequate for the first slice and decision (3) is over-engineering."*

**It does, and it is** — for the bounded first slice. Two independent legs.

### From the code: every alternative arm carries a *different diagnostic code*

`reportRelationError` (`relater.go:4751`) selects the message in a switch at
`relater.go:4780`–`4797`. Each arm above the fallthrough is a distinct code:

| arm | message | **code** |
|---|---|---:|
| `r.relation == c.comparableRelation` | `Type_0_is_not_comparable_to_type_1` | **2678** (`diagnostics_generated.go:1629`) |
| `sourceType == targetType` | `…Two_different_types_with_this_name_exist…` | **2719** (`:1707`) |
| `exactOptionalPropertyTypes` | `…with_exactOptionalPropertyTypes_Colon_true…` | **2375** (`:1103`) |
| string literal → union, suggestion found | `…Did_you_mean_2` | **2820** (`:1907`) |
| fallthrough | `Type_0_is_not_assignable_to_type_1` | **2322** (`:1005`) |

So **filtering on code 2322 already selects the generic branch.** ADR-0040 read
the switch as five ways to render one diagnostic; it is five diagnostics.

And within `SELECTABLE` × `SELECTABLE` each arm is separately unreachable:

- the comparable relation is not what `checkTypeAssignableTo` (`relater.go:340`)
  passes;
- two *distinct* primitives cannot print the same name, and same-name primitives
  are the same type and do not fail;
- a primitive target has no optional properties, so
  `getExactOptionalUnassignableProperties` is empty;
- the did-you-mean arm requires `target.flags & TypeFlagsUnion`, and `UNION` is
  not in `SELECTABLE`.

### From the corpus: **0 of 2,888**

Across every `TS2322` in the judged population, the number of header texts that
are anything other than `Type 'A' is not assignable to type 'B'.` is **zero**.
The probe surfaces unrecognised texts rather than silently bucketing them, so
this is an observation and not an assumption.

### The chain does not affect the suite, and that is worth stating plainly

**1,556 of 2,888** `TS2322`s (53.9%) carry an `errorChain` elaboration. That
sounds like it refutes the above; it does not, for the suite's purpose. The suite
compares `(file, line, column, code)` and **not message text**
(`crates/tsr-conformance/src/diagnostics_suite.rs`), and
`createDiagnosticChainFromErrorChain` (`relater.go:402`) puts the chain *head*'s
code on the diagnostic — 2322 either way. A chain changes fidelity, not pass/fail.

It also cannot occur inside the bound: a chain is built by nested property and
signature comparisons, which primitives have none of. The header type names of a
chained `TS2322` are the *outer* types, which are objects, so those diagnostics
fall outside `SELECTABLE` on the type test alone.

### The one genuine walk output, and it is smaller than the twin

`reportRelationError` generalises a literal source before printing it:
`isLiteralType(source) && !typeCouldHaveTopLevelSingletonTypes(target)` →
`getBaseTypeOfLiteralType`. So `const x: number = "hello"` prints
`Type 'string' is not assignable to type 'number'.`, **not** `Type '"hello"'`. A
naive call-site formatter that prints `TypeToString(source)` gets this wrong.

Measured: **17 of 137** convertible diagnostics print a literal source (the
target could hold a singleton), the other 120 print the widened one. It is a real
rule and it is one `if`, not a reporting twin.

**Verdict: for the `SELECTABLE`-bounded slice, a call-site formatter is faithful,
and ADR-0040's decision (3) is over-engineering *for that slice*.** Decision (3)
remains right for the general case, where the arms are reachable and the chain
carries the elaboration — but "the general case" is the complete relater, which is
the item §1 points at anyway.

---

## 3. Falsifier 3 — position. Mechanism confirmed; the distribution kills the slice

The mechanism ADR-0040 describes is real. `checker.go:5899` passes `node` — the
declaration — as the error node, and `GetErrorRangeForNode`
(`vendor/typescript-go/internal/scanner/scanner.go:2588`) maps
`ast.KindVariableDeclaration` to `ast.GetNameOfDeclaration(node)`. So the reported
column is the **name's**, and an emitter reporting at the initialiser would fail
every case on column while the types were right.

But the distribution is what matters, and it was never measured. Across the 137
convertible diagnostics, resolved to the *node kinds* our parser and binder put at
that offset — not to source text, because an identifier followed by `:` is a
`VariableDeclaration` name **or** a `PropertyAssignment` name and the first
version of this probe conflated 38 of them:

| anchor (parent / node) | diagnostics |
|---|---:|
| `BinaryExpression / Identifier` | **28** |
| `PropertyAssignment / Identifier` | 21 |
| `VariableDeclaration / Identifier` | **18** |
| `ArrayLiteralExpression / *` | 15 |
| `Block / ReturnStatement` | 11 |
| `JsxAttribute / Identifier` | 9 |
| `ElementAccessExpression / *` | 8 |
| `ArrowFunction / *` | 7 |
| `PropertyAccessExpression / *` | 7 |
| `ConditionalExpression / NumericLiteral` | 3 |
| 18 further kinds | ≤ 2 each |

**28 distinct anchors, and the modal one is not the site ADR-0040 names.** Each
row is a different upstream reporting path, not a different node under one path:

- `BinaryExpression` is assignment checking, not `checkVariableLikeDeclaration`;
- `PropertyAssignment` and `ArrayLiteralExpression` are `elaborateObjectLiteral` /
  `elaborateArrayLiteral` (`relater.go:440`'s switch) — which report at
  *sub-positions* and never reach `checkTypeRelatedToEx` at all;
- `Block / ReturnStatement` is return-type checking;
- `JsxAttribute` is `elaborateJsxComponents`.

This is the *level-4* distinction `docs/conventions.md` draws, seen from the other
side: `is_type_assignable_to` is shared machinery, but the **positions** are not,
and the positions are what the suite compares. A working relater plus one call
site converts 15 cases; the other 74 need six more traversal sites, each with its
own elaboration rules.

---

## 4. What was and was not built

**Built:** `crates/tsr-conformance/examples/assignability_shape.rs`, the
instrument, with unit tests on the two functions that could silently inflate the
answer (`is_selectable_name`, `parse_generic_message`).

**Not built:** any diagnostic emitter, any `check_source_file` traversal, any
reporting twin on `relater.rs`. `crates/tsr-checker/src/relater.rs` is unchanged.
ADR-0040's "Nothing is built" line stands.

**Why not, in one paragraph.** The bounded emitter ADR-0040 specifies has a
ceiling of 89 cases (1.6% of the suite) and its named first slice finishes 15
(0.27%), both upper bounds that assume our port already computes the right types
at 28 distinct syntactic positions it currently visits with no traversal at all.
The 89 are gated not by the relater — which handles primitives correctly today —
but by seven separate reporting paths. Meanwhile 364 of the 478 `TS2322`-only
cases (76%) are object-to-object, which is the complete relater, which is the
item ADR-0040 already identifies as "the clean end state" and rejects only as a
*next* step. The measurement says the bounded emitter is not a cheaper route to
the same place; it is a different, much smaller place.

---

## 5. How this would be shown wrong

- **The upper bound is loose in the *other* direction if `SELECTABLE` is not the
  right bound.** The probe classifies on upstream's printed names. If our port
  already produces correct object types for a meaningful share of the 364
  object-to-object cases, the bound is too tight and the sizing is understated.
  `bd tsr-6v7` (widening `SELECTABLE`) is where that would show. The test:
  re-run this probe with the primitive test replaced by "our relater's answer
  agrees with upstream's on both types", which needs the checker and is the
  natural next instrument.
- **15 is wrong if the anchor resolution is wrong.** It resolves the narrowest
  node in *our* AST starting at upstream's reported offset. If our spans differ
  from upstream's for some declaration form, that row is mis-attributed. The
  control is that `VariableDeclaration / Identifier` is exactly what
  `GetErrorRangeForNode`'s `KindVariableDeclaration` arm predicts, and it appears
  with the right magnitude beside anchors that arrive from paths with no
  declaration in them at all.
- **The whole decline is wrong if the seven reporting paths are cheaper together
  than separately.** They share `checkExpression` and the relation walk. If the
  traversal is built once and the six extra sites are each an afternoon, the item
  is 89 cases and not 15. Nothing here measures the *cost* of those sites — only
  that there are seven of them and that ADR-0040 sized the work as one.
- **`0 of 2,888` is falsified by one counter-example**, and the probe prints any
  unrecognised header text rather than bucketing it, so the next run over a moved
  submodule pin will say so.
