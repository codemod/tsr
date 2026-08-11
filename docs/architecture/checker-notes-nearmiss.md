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

### Slice 1 — variable declarations (+4 cases, 0 lost)

Landed. `parse_variable_declaration_list` (`crates/tsr-parser/src/statement.rs`)
now re-tests `is_binding_identifier_or_private_identifier_or_pattern` — a
direct port of `parser.go:6221`, which is `isListElement`'s answer for
`PCVariableDeclarations` (`:871`) — after eating each comma, and records
`NodeFlags::HAS_TRAILING_COMMA` when it stops there.

Measured at the rebase onto `c260de51` (checker-2's §180), one full coverage
run before and after:

| suite | before | after |
|---|---|---|
| `checker_types` cases | 4,697 / 9,538 (49.25%) | **4,701 (49.29%)** |
| `checker_types` lines | 420,818 | 420,823 |
| `diagnostics` | — | **unchanged** |
| `binder_symbols` | 8,444/8,444, 553 skipped for parse errors | 8,447/8,447, **550** |
| `printer_round_trip` | 11,743/11,743, 701 skipped | 11,746/11,746, **698** |

The two 100% suites are the interesting column. Three cases each moved *out of
the skip bucket and into the passing one*: this port had been reporting a parse
error ("Identifier expected") on the manufactured name, and now reports
nothing — which is upstream's behaviour, since upstream's TS1009 for `var a,`
comes from `checkGrammarVariableDeclarationList` (`grammarchecks.go:1648`)
reading the trailing comma off the list, not from the parser at all.

**Residue, named:** TS1009 itself is still unported, so the flag this slice now
sets has no reader in `tsr-checker`. It is recorded anyway because upstream
derives the fact from the list's span outrunning its last child
(`ast.go:137-143`) and this AST cannot — see `NodeFlags::HAS_TRAILING_COMMA`'s
own doc comment.

Two tests, the first a true positive confirmed red under the mutation
`if false && !self.is_binding_identifier...`, the second its control (a comma
followed by a real binding must still continue the list — without it, deleting
the loop body passes).

### Slice 2 — §193, the await context (+4 cases, 0 lost)

Landed, and it is the *opposite* of a list-termination fix: the four cases here
needed this port to stop manufacturing a node in one place and **keep**
manufacturing it in the neighbouring one.

`await` is contextual. Upstream commits to an await expression only when
`isAwaitExpression` (`parser.go:5115`) says so — inside an await context
always, outside one only when the next token is an identifier, keyword or
literal on the same line (`nextTokenIsIdentifierOrKeywordOrLiteralOnSameLine`,
`:4011`). This port had **neither** test and always built an
`AwaitExpression`, over a manufactured missing operand when nothing followed.

The corpus proves each half in a neighbouring pair, and this is the whole
design argument:

| case | source | upstream's assertions |
|---|---|---:|
| `asyncFunctionDeclaration3_es6` | `function f(await = await) {}` | **3** — the initialiser is an identifier |
| `asyncFunctionDeclaration6_es6` | `async function foo(a = await) {}` | **4** — the initialiser IS an await expression, and the fourth line is its missing operand, rendered `> : any` |

So a port with only the lookahead half gets the first right and the second
wrong; a port with neither gets the first wrong and the second right. **This
was measured, not reasoned about**: the lookahead-only version was built first
and came in at **+4 / −4, net zero**, with the four losses precisely
`asyncFunctionDeclaration6/7` and friends plus the `importCallExpression*`
family (`await import(...)`, killed by the same version's `import` deviation).
Net zero is what sent the work to the context flag instead of shipping.

`Parser::in_await_context` is a **bool with save-and-restore**, not a counter
like `no_in`, because the context is *set to a value* at each boundary: a
non-async function nested inside an async one turns it back **off**, which a
counter cannot express. `with_await_context` is upstream's
`saveContextFlags` / `setContextFlags` / restore triple.

Wired at seven boundaries, each anchored at its site: function declarations and
expressions (`parser.go:2506`), class and object-literal methods, arrow
parameters and bodies (`:3299`, `:4484`), class static blocks — which turn it
**on** unconditionally (`:2539`) — and constructors and accessors, which turn
it **off** unconditionally because neither can be `async`.

**Not wired, and named so the gap is visible rather than assumed absent:**

- The **top level of an external module** (`parser.go:554`). Upstream turns the
  context on for a file it has decided is a module, which is what makes
  top-level `await` legal there; this port does not make that decision in the
  parser, so a top-level `await` still goes through the lookahead half.
- **`export =` and `export …`** (`:5117`-adjacent), which upstream parses in an
  await context.
- A **parameter's decorators**, which upstream parses in the *outer* context
  while the rest of the parameter takes the function's (`:3322`).
- The **yield** context, which shares upstream's word. It has no reader here:
  `is_binding_identifier` is upstream's own context-free test, and
  `isYieldExpression`'s context half is unported.

Three tests, all confirmed red under named mutations — one killing the context
half (`if false && self.in_await_context`), one killing the guard entirely
(`if true || self.is_await_expression()`), which is what separates the two
directions the pair above describes.

### Slice 3 — §194, heritage clause elements (+5 cases, 0 lost)

The largest sub-family left after §193, and the first slice where the wall
below turned out **not** to apply.

`isListElement`'s `PCHeritageClauseElement` arm (`parser.go:858-870`) refuses
two tokens this parser used to consume as base expressions:

- **A `{` that is really the class body.** `isValidHeritageClauseObjectLiteral`
  (`:6278`) treats an *empty* `{}` as the base expression only when what
  follows continues the header — `{`, `,`, `extends`, `implements`. A
  non-empty `{` is unambiguous and always an element.
- **An `extends` or `implements` keyword** that is followed by something an
  expression could start with (`isHeritageClauseExtendsOrImplementsKeyword`,
  `:6301`). That is what tells `class C extends implements A` — an `extends`
  clause with **no** types plus a separate `implements` clause, one assertion
  in upstream's baseline — from a class genuinely extending a variable *named*
  `implements`.

**Why the wall did not apply here.** Slice 1's licence was that for
`PCVariableDeclarations` every corpus token failing the element test also
satisfies `isListTerminator`, so a two-way break cannot disagree with
upstream's three-way decision. The same holds for this context, and more
tightly: `isListTerminator(PCHeritageClauseElement)` is exactly `{`, `extends`,
`implements` (`:923`) — the same three tokens the element test refuses. The
recovery arm is unreachable for them by construction, not by coincidence.

Measured: five cases, all in the structural pool
(`parserErrorRecovery_ExtendsOrImplementsClause1/2/3/5`,
`classHeritageWithTrailingSeparator`), **0 lost**.

`isStartOfExpression` and `isStartOfLeftHandSideExpression` came back for this,
now with call sites and with the `import` arm ported properly — its absence is
what cost the lookahead-only await version four cases.

Three tests, each red under a named mutation, the third being the control that
an object literal really can be a base expression (without it, refusing every
`{` passes).

### Slice 4 and after — what the remaining shapes need, and the wall

The other list contexts in the 27 need predicates this parser does not have.
`PCArgumentExpressions` is `token == ... || isStartOfExpression()`
(`parser.go:882`), and `isStartOfExpression` (`:6144`) rests on
`isStartOfLeftHandSideExpression`, `isBinaryOperator` and an `isIdentifier`
that consults the yield and await contexts.

> `isStartOfExpression` **was** transcribed during slice 2 and then deleted
> unused, because the await work took a different road. Its three deviations,
> should it be needed again, all narrowing: no `isBinaryOperator` (only the
> genuinely binary operators are missed — `+`, `-`, `~`, `!`, `<` all start
> unary expressions on their own account); `import` refused outright rather
> than asking `isNextTokenOpenParenOrLessThanOrDot`, **which is the deviation
> that cost the lookahead-only await version four `importCallExpression*`
> cases**; and `is_binding_identifier` in place of `isIdentifier`. The middle
> one is the lesson: a narrowing deviation is safe only where the predicate is
> used to *decline*, and `await import(x)` is exactly where it was not.

More importantly, upstream's loop is a *three-way* decision, not a two-way one.
When the token is neither a list element nor a terminator,
`abortParsingListOrMoveToNextToken` (`:698`) asks `isInSomeParsingContext` —
whether the token would be an element or terminator of any *enclosing* list —
and only breaks if so; otherwise it reports that context's own error, skips one
token, and **retries**. That third arm needs the `parsingContexts` bitmask,
which only exists if `parseDelimitedList` itself is what runs every list.

So slice 1 is exactly the part that needed none of that: for
`PCVariableDeclarations`, every token that fails the element test in the 27
also satisfies `isListTerminator` (`canParseSemicolon()` at `:934`), so the
two-way approximation and upstream's three-way decision cannot disagree. **Any
further slice has to either prove the same coincidence for its context or port
the machinery.** Approximating the third arm would be the kind of
nearly-right recovery that produces a diagnostic upstream never emits, at a
position upstream never names.
