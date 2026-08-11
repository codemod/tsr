# The case-rate board

Notes for `crates/tsr-conformance/examples/nearmiss.rs`. Companion to
`checker-notes-rank.md` (the gradient board) and `checker-notes-cyclegap.md`
(what the gradient board does not attribute).

Read `STATUS.md` §1 first for the numbers this file's sections were measured
against.

---

## §190 The board, and the pool the control found

### The forcing constraint

`checker_types` reports two numbers and **they rank work differently.** The
line gradient is `matched / 478,954`; the case pass rate counts a case only
when *every* line in it is right. §173 measured how far apart they pull: the
largest gradient arm of that window (§167, the missing `RegularExpressionLiteral`
dispatch arm, +328 lines) moved 30 cases — 0.09 cases per line — while a
27-line arm moved 16 cases, **six times the leverage per line.**

Every instrument in `examples/` ranks lines. `rank_board`, `gaproot`,
`depend`, `wrongflip`, `cyclegap` all answer "where are the bad lines", and
in that view a case one line from passing is indistinguishable from a case a
thousand lines from passing. `casedelta` prints the per-case tally but not
*what* the failing lines are, so it can say a case is near and not say what is
in the way. Nothing ranked by the case rate.

`nearmiss.rs` does. Its unit is the case; its `--summary` prints, for each
deficit band, the case rate the suite would report if that whole band
converted.

Measured at `5b1047d`:

| deficit | cases | rate if that band and everything below it converts |
|---|---:|---:|
| 0 (count/section only) | **27** | 46.33% |
| 1 | 784 | 54.55% |
| 2 | 801 | 62.95% |
| 3 | 474 | 67.92% |
| 4 | 413 | 72.25% |
| 5–8 | 951 | 82.22% |
| >8 | 1,696 | 100% |

Baseline: 4,392 / 9,538 = 46.05%.

### The control fired on the first run, and its answer is a pool

The instrument's control is that `passing` must equal the committed snapshot's
passing-case count exactly, since both come from `types_suite::compare`. The
first version read `passing` as `deficit == 0` and printed **4,419** against
the snapshot's **4,392**.

The 27 are not an error in the probe. `compare` fails a case whose *section
count* or *assertion count* differs from the baseline's, independently of
whether the lines it does have are right — and **27 cases match every single
baseline line while rendering a different number of them.** They are the
cheapest cases in the corpus: not one wrong type between them. No other board
here can see them, because every other board ranks lines and these cases have
no bad line to rank.

The rule this earns, and it is the general form of the finding: **a probe whose
verdict is re-derived rather than taken from the gate is measuring something
else.** `deficit == 0` is a plausible reading of "passed" and it is wrong by 27.
Take the gate's own verdict; make the disagreement the control.

### `--counts`: how much of the board is not a checker question at all

The 27 are the pure end of a larger population. `--counts` measures the whole
of it at `5b1047d`:

```
cases rendering MORE assertions than the baseline: 220 (+788 lines)
cases rendering FEWER:                             245
deficit carried by those cases:                    9,353
```

**465 cases — 4.9% of the suite — disagree with upstream about how many
expressions the file has.** Their lines are aligned by position, so from the
first divergence onward every remaining line in the case is compared against
the wrong baseline row. The 9,353 deficit those cases carry is therefore *not*
a count of wrong types, and any type-shaped board that ranks it is ranking
noise. This is the same class of error as the `cycle` ending in `depend.rs`
(`checker-notes-cyclegap.md`): a bucket whose label describes the symptom and
not the cause.

### How you would know this board is wrong

- `--summary`'s `passing` stops equalling the snapshot's. Then the population
  has drifted from the suite's and nothing above is readable.
- The `--counts` populations shrink while the case rate does not move. That
  would mean count agreement is not on any case's critical path, and the
  §191 work below is mis-scoped.

---

## §191 The extra lines are the parser's, and they are one defect

### What the 27 are

`--structural` dumps them. Every one renders **more** lines than the baseline,
never fewer, and they fall into five shapes:

| shape | cases | example |
|---|---:|---|
| an assertion with **empty source text** (`> : any`) | 17 | `parserVariableDeclaration5`, `parserErrorRecovery_ArgumentList2` |
| a keyword rendered as an expression | 4 | `implements : any`, `static : any`, `default : any` |
| a skipped illegal character | 3 | `\ : any` (`parserSkippedTokens13/17/20`) |
| a static block body as an object literal | 1 | `classStaticBlock20` |
| an enum member's computed name | 1 | `parserEnum4` |

The 17 are a missing **node**, not a wrong type. `parserVariableDeclaration5`
is the whole story in one line of source:

```ts
var a,
```

Upstream's baseline records exactly `>a : any`; this port records that and a
second, empty one.

### It is the parser, not the walk — and the check that settles it

The tempting fix is a walker skip: *don't emit an assertion for a zero-width
node*. That is wrong, and the corpus says so directly. Upstream's
`writeTypeOrSymbol` (`type_symbol_baseline.go:344-413`) has **no such guard**,
and `grep -rl '^> : '` over `vendor/typescript-go/testdata/baselines/reference/submodule`
returns a long list of `.types` files that *do* record an empty-text
assertion — `compiler/for.types` among them, which is one of the 27. Upstream
emits these lines when the node exists. It does not emit them here because
**the node does not exist.**

The cause is `parseDelimitedList` (`parser.go:649-704`). Upstream's loop, after
consuming a separating comma, goes back to the top and re-tests
`isListElement(kind, false)`:

```go
list = append(list, element)
if p.parseOptional(ast.KindCommaToken) {
    continue                      // <- back to isListElement
}
```

For `var a,` at EOF, `isListElement(PCVariableDeclarations)` is
`isBindingIdentifierOrPrivateIdentifierOrPattern()` — false — and
`isListTerminator` is true, so the list ends with **one** declaration. This
port's `parse_variable_declaration_list` (`crates/tsr-parser/src/statement.rs:511`)
instead parses unconditionally after each comma:

```rust
loop {
    declarations.push(self.parse_variable_declaration());
    if !self.eat(SyntaxKind::CommaToken) {
        break;
    }
}
```

so it manufactures a declaration whose name is a missing identifier. Every one
of the 17 is that shape in a different list context: variable declarations,
argument lists, heritage clause elements, parameter lists.

### Consequences accepted

This is parser work reached from a checker board, and it is priced in cases
rather than in lines: the 27 are worth **+0.28 points** of the case rate on
their own. The larger claim — that the same defect is inside some of the 220
`--counts` cases that also have wrong types — is stated here as a *hypothesis*
and is not evidence for the work until measured.

The reason it is worth doing anyway is that these 27 are the only pool in the
corpus where the checker is already completely right.
