# The checker

`crates/tsr-checker`, ported from `internal/checker` at the pinned commit
`5b1047d10`. Upstream is **60,269 lines** — a third of the core port, and by
PLAN.md's estimate 120–200 sessions. This document describes what exists, what
does not, and the two decisions that shape everything that follows.

## Status: a foundation, not a checker

Stated first because the distinction matters to anyone reading the conformance
table. `checker_types` reads **0/9,538** and that is correct, not a bug.

**What exists:**

| | |
|---|---|
| `TypeFlags` | Ported one-for-one from `types.go:427`, bit positions included |
| `TypeStore` / `TypeId` | The handle model, per [ADR-0013](../adr/0013-checker-memoisation.md) |
| Intrinsics | Created in upstream's order (`checker.go:975`–`1015`) |
| Literal & keyword expression types | With interning |
| `type_to_string` | For the above, in `.types` baseline form |
| **Declaration types** | `getTypeOfSymbol` for variables, parameters and properties (`bd tsr-4sc.2`) |
| **Type nodes** | `getTypeFromTypeNode` for the keyword, literal and parenthesised forms |
| **Literal freshness** | The `freshType`/`regularType` pair, which is what makes `const x = "a"` be `"a"` and `let x = "a"` be `string` |
| **Circularity detection** | The resolution stack, `pushTypeResolution`/`popTypeResolution` |
| **Identifier references** | Resolve the name, take the symbol's type — with no narrowing |

**What does not exist:** object/union/intersection/generic/conditional/
indexed-access types, type references (so `interface I` is unusable as an
annotation), functions, classes, enums, accessors, aliases, destructuring,
assignability, inference, overload resolution, control-flow narrowing, and every
one of the checker's diagnostics.

`checker_types` still reads **0/9,538**, and that is correct: the checker can now
type a declaration, but nothing renders those types in `.types` baseline form.
That producer is `bd tsr-4sc.3`, and it is what will first move the number.

## One module per upstream concern, not one file

Upstream keeps the whole checker in a single 60,269-line `checker.go`. That is
not a shape worth reproducing, and it stopped being workable here at ~1,200
lines: every ranked item on the histogram below lands in the same file, so any
two people working the ranking in parallel collide on it.

`crates/tsr-checker/src/` is therefore split by **upstream concern**, so a
reader who knows `checker.go` can find the arm they want:

| module | upstream | answers |
|---|---|---|
| `checker.rs` | the `Checker` struct and its links | what the checker remembers |
| `expressions.rs` | `checkExpression` | the type of an expression |
| `binary.rs` | `checkBinaryLikeExpression` | `a + b`, `a === b`, `a = b` |
| `members.rs` | `checkPropertyAccessExpression`, `getPropertyOfType` | `a.b` |
| `symbols.rs` | `getTypeOfSymbol` | the type a *value* symbol has |
| `declared.rs` | `getTypeFromTypeNode`, `getDeclaredTypeOfSymbol` | what a type node and a *type* symbol denote |
| `literals.rs` | `getWidenedLiteralType` and its pair | fresh versus regular |

Rust allows several `impl` blocks on one type across modules, so this is a
file-boundary change and not a design change: the methods, their names and
their bodies are untouched. The only substantive edit is that `Checker`'s
fields became `pub(crate)`, because each module writes to a memo.

**The rejected alternative was splitting by size** — carving the largest
functions out into `checker_2.rs` and so on. It would have been quicker and it
would have made the map useless: the value here is that `getTypeOfSymbol` and
`getDeclaredTypeOfSymbol` are in *different* files, because conflating those two
questions is the specific mistake this document already warns about twice.

The split was verified rather than assumed: `types_shapes --release` was run
before and after and its whole output diffed **byte-identical**, 173,260/468,921
either way. A refactor that moves a number is a bug, and that is checkable in one
command.

## Reaching a declaration from a symbol — the blocker, now cleared

Found 2026-08-05 on starting `bd tsr-4sc.2`, and structural rather than a matter
of writing more code: **nothing mapped a `NodeId` back to a typed node.**

A binder `Symbol` holds `value_declaration: Option<NodeId>`. Computing its type
means reading that declaration's annotation and initialiser, which live in the
typed node — `VariableDeclaration<'a>` — while `NodeTable` stores only kind,
span, flags and parent (`crates/tsr-ast/src/lib.rs:159`–`186`). So the checker
could reach a declaration's *position* and not its *contents*. Upstream never
meets this: a Go `*ast.Symbol` holds a real `*ast.Node`, and the question does not
arise. Here it is [ADR-0003](../adr/0003-tree-plus-side-tables.md) meeting
upstream's design.

`ParsedSourceFile::node_map` now answers it — `NodeMap::get(id) -> Option<Node>`.

Three candidate structures were measured over the four benchmark fixtures
(`crates/tsr-binder/examples/node_lookup.rs`, each option in its own process;
419,572 nodes, 41,831 symbols):

| option | entries | exact KiB | RSS Δ KiB | populate | answers `parent(id)`? |
|---|---:|---:|---:|---:|:--:|
| dense `Vec<Option<Node>>` by `NodeId` | 419,565 | 6,555 | 6,656 | 4.02 ms | yes |
| per-symbol, one `Node` per symbol | 36,590 | 653 | 1,488 | 5.55 ms | no |
| sparse `FxHashMap` of declarations | 43,297 | 1,432 | 3,156 | 9.05 ms | no |

**Resolved by [ADR-0032](../adr/0032-reaching-a-typed-node-from-an-id.md), then
superseded the same day by
[ADR-0033](../adr/0033-the-parser-fills-the-node-map.md).** The dense shape won
on capability — the other two answer only "the declaration node of this symbol",
while `nodes.parent(id)` returns an id that 1,134 sites in upstream's checker
need resolved to a node. What changed is *who fills it*.

ADR-0032 put the fill in the binder, believing it free there. Two measurements
said otherwise: the bind walk reaches only 417,837 of 419,565 nodes (0.41% short
— `case`/`default` keywords and some zero-width `ForOfStatement`s, because
`push_children` includes token-valued fields and `bindChildren` does not), so it
needed a separate pre-pass; and that pre-pass cost **+16.1%** on `checker.ts`
parse+bind, mostly zeroing a 4.8 MB vector on every bind.

The parser fills it instead (`tsr_ast::NodeMap`, `bd tsr-4sc.5`). `NodeTable`
hands out ids sequentially and the parser allocates the typed node immediately
after, so recording is a `Vec::push` — no zeroed allocation, no second walk, and
coverage complete by construction rather than by argument. Measured against the
binder version: parse+bind **−17.0%** on `checker.ts`, binder memory back to
14,848 KiB, total bytes-per-source-byte 8.73 → 8.68. The cost lands on parse-only
consumers instead: **+7.4%** parse time and +6,400 KiB of AST, worth **+0.9%** on
a whole-corpus conformance run.

Two predictions this repository got wrong along the way, both corrected in the
records rather than quietly: the dense table is the *fastest* to populate despite
being ten times the size (it stores by index; the others probe a hash map per
node), and the cheap options' resident cost is 2.2–2.3× their byte counts because
both need a transient set during the walk — so the real spread is 4.5×, not 10×.

## Two things upstream does that the issue text got wrong

Both found by reading `checker.go` rather than by trusting the summary, and both
recorded here because the summaries are what the next session will read first.

- **A circularity does not simply become `errorType`.**
  `reportCircularityError` (`checker.go:18822`) returns `errorType` only when the
  declaration has a *type annotation* that references itself; a circular
  *initialiser* reports a different diagnostic and returns **`anyType`**. Those
  print identically and behave differently, which is precisely the trap this
  crate already documents under "distinct types that print the same string".
- **The circularity mechanism is a stack, not a per-symbol flag.**
  `pushTypeResolution`/`popTypeResolution` (`checker.go:18758`) keep a
  `typeResolutions` stack keyed by (entity, property name), and on finding a
  cycle they mark **every frame from the cycle start onward** as failed — so all
  participants in the cycle resolve to an error, not only the symbol that closed
  it. A boolean "currently resolving" flag per symbol does not reproduce that.

## What the declaration slice does *not* do, stated precisely

Three limits worth knowing before reading a `checker_types` failure, because each
produces a plausible-looking wrong answer rather than an obvious gap.

- **No control-flow narrowing.** `checkIdentifier` is ported only as far as
  "resolve the name, take the symbol's type", so a reference yields the
  *declared* type. Upstream reaches `getFlowTypeOfReference` here, so a reference
  to `let x = "a"` narrows to `"a"` where the assignment dominates; ours says
  `string`. The binder builds the flow graph already and nothing reads it.
- **The annotation branch of `reportCircularityError` is unreachable today.**
  A circular *initialiser* returns `anyType` and is tested end to end; a circular
  *type annotation* returns `errorType` and cannot yet be triggered, because the
  type nodes that could close such a cycle — `typeof x`, a type reference — are
  unported and yield `errorType` without recursing. The branch is written because
  it is upstream's behaviour, and it is recorded here as untested rather than
  presented as working.
- **The symbol-flags dispatch is currently redundant.** `getTypeOfSymbol` tests
  `SymbolFlags::VARIABLE | PROPERTY` before taking the variable path, exactly as
  upstream does. For every symbol shape this slice reaches, deleting that test
  changes nothing — a non-variable symbol's declaration kind is rejected by the
  worker's match and lands on the same `errorType`. Mutating it turns no test
  red, and that is stated rather than papered over with a test that would not
  bite.

## A correction the tests forced

The first version of `a_reference_takes_the_type_of_what_it_resolves_to`
asserted that `const a = "x"; let b = a;` gives `b : "x"`, on the assumption that
a *reference* could not carry a fresh literal type. It gives `string`, and the
implementation was right.

**Freshness propagates through a `const`.**
`getWidenedLiteralTypeForInitializer` (`checker.go:16897`) returns the
initialiser's type *unchanged* when the declaration is constant — and that type
is the fresh literal `checkExpression` produced. So `a` holds a fresh `"x"`, and
the `let` still has something to widen. The contrast that pins it is
`let a: "x" = "x"; let b = a;`, where `a`'s type came from a type *node* and is
therefore regular, so `b` stays `"x"`.

That contrast is also the only thing in the suite that distinguishes freshness at
all: deleting the freshness check from `getWidenedLiteralType` turned **no test
red** until that case was written, because every other path either starts from a
fresh literal or returns an annotation without widening.

## The order the rest is built in, and why

**Re-ranked 2026-08-05 against a measurement, replacing a ranking made from
answer shapes alone.** The shape table is kept because it is still the honest
picture of the *target*; what changed is that it ranked the work badly, and the
section after it says why. The instrument is `examples/types_shapes.rs`
(`bd tsr-4sc.6`), described in [checker-oracle.md](checker-oracle.md).

Shares are over the **468,921 assertion lines the walker aligns**, which is the
population `checker_types` judges. An earlier count over all 512,800 corpus lines
gave 35.30% / 18.86% / 14.25% / 12.96% for the first four rows; the differences
are the different population, not a corrected number.

| upstream's answer | share | we get it right | misses: unported | misses: wrong |
|---|---:|---:|---:|---:|
| intrinsic (`string`, `number`, `any`…) | 35.06% | **23.54%** | 118,484 | 7,219 |
| literal (`"a"`, `1`, `true`) | 19.55% | **74.24%** | 23,000 | 620 |
| named type reference (`C`, `M.I`) | 13.44% | 0.74% | 61,251 | 1,298 |
| function/signature (`() => void`) | 11.82% | 0.00% | 55,182 | 239 |
| object literal type | 5.69% | 0.00% | 26,505 | 181 |
| `typeof X` | 3.39% | 0.00% | 15,868 | 44 |
| generic reference | 3.20% | 0.00% | 14,861 | 132 |
| union | 2.91% | 0.00% | 12,436 | 1,212 |
| array | 2.66% | 0.00% | 12,072 | 419 |
| other / intersection | 2.27% | 0.00% | 10,525 | 135 |

*Unported* is a line we answered `errorType` on; *wrong* is a line we answered
something else on and disagreed. The split is readable only because this port
renders `errorType` as `error` and not as `any` — see the correction below.

### The 54% → 22% question, answered

We nominally cover intrinsic + literal = 54.6% of lines and the gradient reads
22.39%. The shortfall is **not spread across the two buckets**:

- **Literals are 74.24% right** and close to done; their residual is
  overwhelmingly gaps rather than wrong answers (23,000 against 620).
- **Intrinsics are 23.54% right**, and that one row is essentially the entire
  shortfall. 34% of the intrinsic bucket is upstream answering `any` — 55,976
  lines — which is a computed answer (an unannotated parameter, an error type
  flowing outward) and not a free win.
- **Every other shape is 0.00%.** Nothing is scoring by accident, and the 107,238
  matched lines are all in the two claimed buckets.

### What a bucket cannot say, and what the histogram says

A bucket names the *answer*, never the work: `a + b` → `number` is an intrinsic
answer that needs binary-operator checking. So the same run reports where the
checker stopped on each of the 350,184 `error` lines.

| where it stopped | lines | share of gaps |
|---|---:|---:|
| an expression we do not compute | 141,036 | 40.27% |
| a type node we cannot resolve (an annotation) | 73,687 | 21.04% |
| an initialiser expression we do not compute | 48,095 | 13.73% |
| a symbol kind `getTypeOfSymbol` does not handle | 43,732 | 12.49% |
| a member name resolved as if it were free (`bd tsr-tl8`) | 21,939 | 6.26% |
| other | 11,160 | 3.19% |
| **a free name that does not resolve** | **10,535** | **3.01%** |

The commonest individual stops:

```text
BinaryExpression                                             39,035
PropertyAccessExpression                                     23,732
CallExpression                                               15,867
ElementAccessExpression                                      13,549
Parameter, annotation TypeReference               10,157 +    6,217
VariableDeclaration, initialiser ArrayLiteral                10,807
FunctionDeclaration / ClassDeclaration, no type at all  7,485 + 7,227
ModuleDeclaration, no type at all                             5,569
```

**Three of those change the plan.**

1. **Lib files are not the blocker they were ranked as.** Names that do not
   resolve at all are 3.01% of gaps, and the commonest is `undefined` (1,675) —
   an intrinsic, not a global. `Symbol`, `console`, `Promise`, `Object`, `Math`
   and `Array` together are under 1,500 lines. `bd tsr-9or.1` was ranked third on
   the reasoning that *"without `lib.d.ts` every reference to a global is
   `errorType`"*; measured, that is worth ~2% of aligned lines **directly**. It
   remains necessary — most of what a global would unlock sits behind the
   expression work below, so this is a lower bound and not a verdict — but it is
   not the next move.
2. **The largest item was not on the list at all.** The remaining expression
   forms are 40% of gaps and had no ranked entry, precisely because no answer
   shape corresponds to them. This is the failure mode the bucket table was
   documented as having, now observed rather than hypothesised.
3. **Named references are 13.44% of lines at 0.74%, and the road to them runs
   through annotations.** 73,687 gap lines are a declaration whose *annotation*
   we cannot resolve, dominated by `TypeReference` — `getTypeFromTypeNode` into
   `getDeclaredTypeOfSymbol`, which is `bd tsr-4sc.7`, and it pays in the
   named-reference and intrinsic buckets both.

### The ranking that follows

**Superseded 2026-08-05 by the re-ranking below, after three of its items
landed.** Kept in one line each so the predictions can be checked against what
happened: expressions first (`bd tsr-4sc.13`), then declared types
(`bd tsr-4sc.7`), then `getTypeOfFuncClassEnumModule` (`bd tsr-4sc.8`), the
member-name defect (`bd tsr-tl8`), printing (`bd tsr-4sc.1`), lib files
(`bd tsr-9or.1`), unions and `typeof` (`bd tsr-4sc.9`, `.10`), narrowing
(`bd tsr-4sc.11`), per-configuration runs (`bd tsr-bb4.1`).

The first two were done, in that order, and moved the gradient 22.39% → 34.92%
with +309 cases. **The ranking predicted the direction and understated the
size**, which is the outcome a ranking is supposed to have.

## The re-ranking, 2026-08-05

Taken from the histogram at `72d148a`, not from the list above. Gap totals are
over the 287,990 assertion lines this port still answers `errorType` on, of
468,921 aligned.

| where the checker stops | lines | share of gaps |
|---|---:|---:|
| an expression we do not compute | 112,230 | 38.97% |
| a type node we cannot resolve | 53,926 | 18.73% |
| an initialiser expression we do not compute | 44,656 | 15.51% |
| a symbol kind `getTypeOfSymbol` does not handle | 35,488 | 12.32% |
| a member name resolved as if it were free (`bd tsr-tl8`) | 21,939 | 7.62% |
| a free name that does not resolve | 10,535 | 3.66% |
| other | 9,202 | 3.20% |

And by the shape of upstream's answer, the buckets still reading **0.00%**:

```text
55,161  function/signature   11.82% of all aligned lines
24,344  object literal        5.69%
15,866  typeof                3.39%
12,051  array                 2.66%
12,026  union                 2.91%
```

### 1. Members on object types, and property access

Expressions are 54.5% of gaps once initialisers are counted with them, and the
largest single stop inside that is `PropertyAccessExpression` — 23,732 lines.
It is blocked on the piece of `bd tsr-4sc.7` that was deliberately left out:
class, interface and object types currently have **identity and a printed form
and no members**. That was sound while nothing looked inside a type; property
access is exactly the thing that looks inside.

This is one item and not two. Porting members without property access scores
nothing, and property access without members cannot be written.

### 2. `getTypeOfFuncClassEnumModule` (`bd tsr-4sc.8`)

35,488 gap lines are a symbol whose kind `getTypeOfSymbol` does not handle:
7,485 function declarations, 7,227 classes, 5,569 modules, 5,686 methods. It is
also the only route to the **largest answer bucket still at zero** —
function/signature, 55,161 lines and 11.82% of every aligned line — and to
`typeof X` (15,866), which is what a class or module symbol's type prints as.

### 3. A class or interface's type parameters do not *resolve*
(`bd tsr-y4u.21`)

**Corrected 2026-08-05. The heading of this section used to read "The binder
does not scope a class or interface's type parameters", and that diagnosis was
wrong.** The lines are real; the cause named here was not. Recorded rather than
silently edited, because the wrong turn is the useful part: this is the eighth
time in this project that an expectation and the implementation disagreed and
**the implementation was right**.

Of the 8,229 gap lines from names in *type* position that do not resolve, the
four commonest names are `T` (1,841), `U` (233), `V` (161) and `K` (95) — 2,330
lines.

The original probe used `lookup_local`, which reads only a container's `locals`
map, and concluded that `T` was bound nowhere for a class or interface. Probing
the `members` table as well (`crates/tsr-binder/examples/type_parameter_scope.rs`)
gives the opposite answer:

```text
interface I<T> { p: T }          members[T] of `I`,   T.parent = I
class C<T> { p: T; }             members[T] of `C`,   T.parent = C
declare class D<T> { m(p: T) }   members[T] of `D`,   T.parent = D
const E = class<T> { … }         members[T] of `__class`
```

**The binder is correct and matches upstream exactly.**
`declareSymbolAndAddToSymbolTable` switches on the *container* kind and sends
`KindClassDeclaration`/`KindClassExpression` to `declareClassMember` →
`GetMembers(symbol)`, and `KindInterfaceDeclaration` to `GetMembers(symbol)`
(`internal/binder/binder.go:429`–`441`). ADR-0023 already landed this. Upstream
even comments on the oddity at `symbolaccessibility.go:766`: *"Type parameters
are bound into `members` lists so they can merge across declarations. This is
troublesome, since in all other respects, they behave like locals :cries:"*.

**The real defect is in name resolution.** `BindResult::resolve`
(`crates/tsr-binder/src/lib.rs:243`) walks parents consulting `locals` only.
Upstream's `(*NameResolver).Resolve` (`internal/binder/nameresolver.go`) has a
`KindClassDeclaration | KindClassExpression | KindInterfaceDeclaration` arm that
looks the name up in `getSymbolOfDeclaration(location).Members`, filtered by
`meaning & SymbolFlagsType`, with two rules attached: the symbol must be a type
parameter declared in *this* container, and a reference from a `static` member is
an error rather than a resolution (TS 1.0 spec §3.4.1). That arm is missing.

Fixing it requires a `meaning: SymbolFlags` parameter on `resolve`, because a
class's `members` table holds its properties and methods too — returning a type
parameter for a *value* reference would be answering a question upstream does not
ask.

Still ranked third, and for the same reasons as before: it is cheap, it is a
*defect* rather than an absence, and item 1 walks straight into it — every member
type that mentions `T` is a gap until it is fixed. `binder_symbols` reads 98.03%
and cannot see it, which is the fourth entry in this document's list of things
that suite sits through.

**What this cost.** Nothing was built on the wrong diagnosis, because the teammate
who owned it probed before writing. Had they not, the work would have gone into a
binder that was already right.

### 4. A program: lib files **and** the other files of a case (`bd tsr-9or.1`)

These were separate concerns and the measurement says they are one item, because
they are one missing object: there is no program, so every file is parsed, bound
and checked entirely on its own.

- **Lib.** Of the twenty commonest unresolved *type* names — 5,897 of those 8,272
  lines — `Promise` (645), `Object` (369), `Array` (304), `Record` (212),
  `Number` (189), `Partial` (142), `Readonly` (104) and `Iterable` (89) are lib
  types: 2,054 lines. The **array bucket** (12,051 lines, still 0.00%) is
  structurally the same request: upstream models `T[]` as a reference to the
  global `Array` interface, which is why this port should *not* answer it with
  another type that merely prints alike.
- **Multi-file cases.** 1,091 of 9,538 judged cases (11.44%) have more than one
  file section. They carry 26,479 aligned lines (5.65% of the total) and read
  **24.50% right against 35.67% overall** — a name declared in one file of a case
  cannot resolve from another.

`bd tsr-4sc.6` demoted lib files from third on a 3.01% direct-effect number and
said in terms that it was a lower bound. Two layers have since been ported and
the bound has not moved — the unresolved-name count is still 10,535 — but the
work standing between it and the score has. It is ranked fourth rather than
first because items 1 and 2 are three to ten times larger and neither needs it.

**Corrected 2026-08-05: that paragraph reasons from a number the instrument could
not move, and lib is 1.75× larger than it says.** The roll-up row labelled *"a
free name that does not resolve (lib files, `bd tsr-9or.1`)"* only ever contained
value-position failures — a name failing in *type* position is caught four
branches earlier by the annotation test and filed under "a type node we cannot
resolve". Measured against the 2,279 names the 108 bundled `.d.ts` files declare:

| | lines |
|---|---:|
| the row's 10,535, of which lib actually declares | **3,126** |
| …and does not (`undefined`, `div`, `a`, `b`, `x`, `_`) | 7,409 |
| lib names in type position, filed elsewhere | 3,210 |
| the array bucket — `T[]` is a reference to the global `Array` | 12,051 |
| **lib's direct effect** | **18,387**, 6.52% of gap lines |

So "the bound has not moved across two ported layers" was never evidence: the
counter sat where those lines could not arrive, and porting layers moves lines
between the *other* categories without touching it. The full record is in
[checker-oracle.md](checker-oracle.md#the-second-instrumentation-finding-a-roll-up-row-was-labelled-with-an-issue-it-mostly-did-not-contain);
18,387 is an upper bound, for the reason stated there.

This does not reorder items 1 and 2 — 35,488 and its 55,184-line bucket are still
larger — but it puts `bd tsr-9or.1` **above unions** (11,829) rather than below,
and it means the array bucket is not a separate item to be ranked at all.

### 5. Unions (`bd tsr-4sc.9`), then narrowing (`bd tsr-4sc.11`)

12,026 gap lines directly, plus the logical operators left as gaps by
`bd tsr-4sc.13`, plus `boolean` ceasing to be a fake intrinsic, plus the enum
divergence recorded above, which must be **replaced** by a real union rather than
extended. Narrowing still adds no bucket of its own and still cannot be seen by
this histogram; it becomes measurable once 1 and 2 land.

Unchanged from the previous ranking and repeated only so they are not lost:
`bd tsr-tl8` (the member-name defect, 21,939 lines and the one place this port
can answer *wrongly* where a gap belongs), `bd tsr-4sc.1` (printing — literals
are 87.22% right and 721 of their misses are wrong answers rather than gaps),
and `bd tsr-bb4.1` (per-configuration runs, still worth doing only when the rate
makes the denominator matter).

## The binary operators, and one deliberate deviation

Ported 2026-08-05 (`bd tsr-4sc.13`, first slice). `checkBinaryLikeExpression`
(`checker.go:12336`) is a dozen unrelated rules behind one node kind, and they
do not become available at the same time, so the port is by arm and the order
came from the histogram rather than from upstream's source order:

| operator | corpus lines lost | now |
|---|---:|---|
| `=` | 18,743 | the right-hand type, freshness intact |
| `+`, `+=` | 3,537 | number / bigint / string, in upstream's order |
| `===` `==` `!==` `!=` `<` `>` `<=` `>=` `in` `instanceof` | 6,336 | `boolean` |
| `* ** / % -` `<< >> >>>` `\| ^ &` and their `=` forms | 6,886 | number, or bigint |
| `,` | 490 | the right-hand type |
| `&&` `\|\|` `??` | 1,860 | **still a gap** — a union of the operands (`bd tsr-4sc.9`) |

Measured after: `checker_types` 278 → **313** cases, gradient 22.39% →
**28.72%**. The intrinsic bucket went 23.54% → 34.82% right and the literal
bucket 74.24% → **87.13%**, the latter almost entirely from `=` and `,`
returning the right-hand type *unchanged* — a fresh literal survives an
assignment, so `x = "a"` is `"a"` and not `string`.

Two arms are worth stating because they look like shortcuts and are not. **Every
comparison is `boolean` whatever its operands are**: upstream computes the
operand types only to report on them and returns `booleanType` unconditionally,
so these answer even where an operand is a gap, and `f() === g() : boolean` is a
computed result rather than a guess. And **assignment returns the right-hand
type**, not the declared type of the left — the rest of upstream's arm is
`checkAssignmentOperator`, which reports and does not compute.

### An `errorType` operand propagates, where upstream would answer `number`

Upstream's arithmetic arm begins *"if both are any or unknown, assume the
operation resolves to `number`"*, and `errorType` carries `TypeFlagsAny`, so
`unknownThing * 2` is `number` upstream. That is sound **there**, where
`errorType` means an error was already reported and the operand really could be
anything.

In this port `errorType` also means *an unported form*, and the same rule would
convert a gap into a claim: `f() * 2` would read `number` whether or not `f`
returns a `bigint`, and — worse — `examples/types_shapes.rs` could no longer tell
the two apart, which is the instrument the whole ranking above is built on. So an
`errorType` operand propagates. Upstream does exactly this in its `+` arm
(`checker.go:12452`), which is the precedent for the shape of the deviation if
not for its scope.

**The cost was measured rather than asserted.** Following upstream instead —
error operand in, `number` out — reads `313 / 29.87%` against this port's
`313 / 28.72%`. So the deviation costs **1.15 gradient points, about 5,400
lines, and no cases at all**. That is the price of the gap/wrong split being
trustworthy, and it is a price that shrinks on its own: every expression form
ported removes operands from the population it applies to.

**How this would be shown wrong.** When property access, call and element access
land, the operands mostly stop being `errorType`. If a large population still
reaches the arithmetic arm with an `errorType` operand then, the gap is not the
operand's form but something else, and this should go back to upstream's rule.

Two guards in the ported code are **currently unobservable**, and both say so in
place rather than being covered by a test that would not bite: the
destructuring-assignment check (an array or object literal is unported, so both
paths reach `errorType` anyway) and the order of `+`'s numeric and string tests
(with only primitive types, nothing is assignable to both kinds). Each becomes
load-bearing with a named future change, which is why they are kept — the same
reasoning as the symbol-flags test in `getTypeOfSymbol`.

## Named types: what a type reference resolves to

Ported 2026-08-05 (`bd tsr-4sc.7`, first two slices), on the histogram's second
ranked item — 73,687 gap lines whose declaration carried an annotation this port
could not resolve.

| slice | `checker_types` | gradient |
|---|---|---|
| *(before)* | 313 | 28.72% |
| `getDeclaredTypeOfSymbol` + type references | 554 | 33.57% |
| anonymous object types | 571 | 34.06% |
| generic references | 587 | 34.92% |
| members, property access and `this` | **596** | **36.17%** |

The named-reference answer bucket went from **0.74% to 34.80%** right, and 241
whole cases landed on the first slice — the largest case movement so far.

### What each kind of symbol declares

`getDeclaredTypeOfSymbol` (`checker.go:23670`) in upstream's dispatch order:
class and interface, type parameter, type alias, enum. Enum members and
`import X = ...` aliases remain gaps.

The one that is *not* obvious, and was taken from the baselines rather than
guessed: **a type alias is transparent.** `conformance/typeAliases.types` records

```text
type T1 = number;
>T1 : number
var x1: T1;
>x1 : number
```

so the alias name does not survive into the printed type. Printing `T1` would
look more informative and be wrong. The exception is a *generic* alias —
`type Tree<T> = ...` records `>Tree : Tree<T>` — which needs upstream's
alias-symbol machinery and is a gap.

Two further forms come from the corpus rather than from reasoning: a class
declaration name records its **instance** type (`class A {}` → `>A : A`, not
`typeof A`), and a generic one records its own parameters (`>C : C<T>`).

### `getTypeOfNode` asks three questions, in an order that matters

The producer's `type_at_location` now follows `checker.go:31927`: a **type
declaration's own name** takes `getDeclaredTypeOfSymbol`, any other declaration
name takes `getTypeOfSymbol`, and an expression takes `checkExpression`. Testing
the general declaration-name branch first would answer every class name with
`typeof A` — a plausible line, and wrong in every one of the 7,227 class
declarations in the corpus.

### Three divergences, all of them visible here rather than in the code

1. **An enum's declared type is a named type, not a union.** Upstream builds the
   union of its members' literal types (`checker.go:23874`), which happens to
   print as the enum's name. Without unions (`bd tsr-4sc.9`) this port creates a
   type that prints the same string and has none of the behaviour. The printed
   line is right; nothing else about it is. It must be **replaced** when unions
   land, not extended.
2. **The printed name is computed once, at creation.** Upstream's node builder
   renders a name from the type's symbol under scoping rules this port has no
   equivalent of. Identity is unaffected — one type per symbol, via the
   `declared_types` memo — so two same-named declarations in different scopes
   are still different types that happen to print alike. It stops being adequate
   as soon as a name needs qualifying or shadowing.
3. **A class or interface type has no members.** Upstream's
   `getDeclaredTypeOfClassOrInterface` builds members, base types and a `this`
   type; this builds identity and the printed form. Nothing depends on the
   members yet because no relation is computed, so a type this port cannot look
   inside is still the right answer to *"what type is this"* — and that stops
   being true the moment property access lands.

### An anonymous object type prints its members, or it is a gap

`{ a: string }` has no symbol to be named by, so it prints structurally —
`{ a: string; }`, with the spaces and the trailing semicolon upstream's printer
emits, and `{}` when empty. Any member this port cannot render — a method, a
call or index signature, an accessor, a computed name, or a property whose own
type is a gap — makes the **whole type** a gap. A partial object type is a wrong
answer dressed as a right one, and it fails the line either way.

**This slice paid less than its bucket, and the reason is worth recording.**
Object-literal answers are 5.69% of assertion lines, and porting the *type* node
moved that bucket only to 7.79% — because most of those lines are produced by
object literal **expressions**, which are still unported. It is the same lesson
the ranking was built on, arriving from the other direction: a bucket names the
answer's shape and not the feature that computes it. `{ a: string; }` on an
annotation and `{ a: string; }` inferred from `{ a: "x" }` are one bucket and two
features.

### Members, property access, and `this`

Ranked first by the re-ranking above, and the first item whose *measured* result
disagreed with its estimate. 571 → 596 cases, 34.06% → 36.17%.

Object types now carry the symbol whose members table the binder already built,
so `getPropertyOfType` is a lookup in it; `checkPropertyAccessExpression` is the
receiver's type, that lookup, and `getTypeOfSymbol` on what it finds. `this`
inside a class is the class's **`this` type** — printed `this`, not `C`, which
is what the corpus records — carrying the class's members, so `this.x` resolves.
Arrow functions are transparent to `this` and plain functions are not.

**`bd tsr-tl8` is closed by this.** The `b` of `a.b` is now typed as the access,
the way upstream reaches it by recording the resolved symbol on the name node.
The lines where a member name was resolved as a *free name in the enclosing
scope* — the one place this port could answer wrongly where a gap belonged —
went from **21,939 to 74**.

#### The estimate was 23,732 lines and the result was a quarter of that

Property access was the largest single stop in the corpus and porting it moved
the gradient about a point. The histogram says why, and the answer is worth more
than the slice:

```text
36,678  13.00%  a property access whose receiver we cannot type
 8,608   3.05%  a property access whose property we cannot find or type
```

Of the receivers: 14,803 are identifiers whose symbol has no type, 1,953 are
themselves property accesses, 471 are calls. When the receiver *can* be typed the
lookup succeeds — only 886 lines are "no such property" on a receiver we typed,
and 298 on a `this`. **The property lookup was never the blocker; the receiver
was.** That number could not have been taken before this landed, because until a
property access could succeed there was nothing to attribute its failure to.

What it points at is item 2 of the ranking, unchanged and now underlined:
`getTypeOfSymbol` for function, class and module symbols (`bd tsr-4sc.8`) is what
gives those 14,803 identifiers a type. Inherited members are the other half —
a base class's properties are not in the derived symbol's table, so
`class C extends B {}` finds nothing of `B`'s.

#### Two limits recorded rather than tested

An instantiated `C<number>` carries **no** members, because they would be `C`'s
uninstantiated ones and `c.a` would read `T` where upstream reads `number`. That
is blunter than upstream and it costs answers: a member whose type never mentions
a type parameter is the same before and after instantiation, and upstream answers
it. The correct rule is `couldContainTypeVariables`, which arrives with real
instantiation. There is deliberately **no test pinning the current answer** — it
is the worse of the two, a test would cement it, and the obvious test cannot
distinguish them anyway while `bd tsr-y4u.21` keeps a class's type parameters out
of every scope.

An early draft guarded against an `errorType` receiver before looking a property
up. No mutation could make that guard observable — `errorType` is an intrinsic and
never carries a members table, so the lookup misses anyway — so it was removed
rather than kept as decoration.

### A generic reference carries its arguments, and substitutes nothing

`C<number>` is the target symbol plus its resolved arguments, interned on that
pair so the same instantiation written twice is one type. The generic-reference
answer bucket went **7.30% → 34.86%** right and the gradient 34.06% → 34.92%.

A generic *alias* keeps its own name, unlike the transparent non-generic case:
`type Tree<T> = T | { left: Tree<T> }` records `>Tree : Tree<T>`
(`conformance/genericTypeAliases.types`). The body is not expanded, which is
also why a self-referential alias terminates here.

**No substitution means no depth limit, and that is deliberate.** Upstream
guards `instantiateType` with a depth of 100 and a count of 5,000,000
(`checker.go:22111`) because self-referential generics generate new type
identities forever. This port has no members to substitute *into*, so the only
recursion is over the source nesting of the argument type nodes, which is finite
in a parsed file. Porting the limits now would be a guard around a loop that
does not exist — it would read as coverage and provide none. `bd tsr-el3.2`
records that they belong with `instantiateType`, and they are not optional then.

Two arities are distinguished, because only one of them is upstream's error:
outside `[minTypeArgumentCount, len(parameters)]` upstream reports and answers
`errorType`; *inside* it, missing arguments are filled from the parameters'
defaults, and a default may reference an earlier parameter — which is
substitution, so that case is a gap rather than a guess.

### What is left under `bd tsr-4sc.7`

From the histogram, after this work:

```text
24,344  object-literal answers, nearly all from expressions not annotations
12,051  array types
 9,506  generic references that still fail — largely names that do not
        resolve, which is the next point
        qualified names (`M.I`), needing resolveEntityName through exports
```

**The lib-file argument has strengthened, and the measurement says so.** When
`bd tsr-4sc.6` demoted `bd tsr-9or.1` it noted the 3% figure was a *direct-effect
lower bound*, because most of what a global unlocks sat behind unported
expressions. Two of those layers are now ported, and what remains in front of
the array bucket (12,051 lines, still 0.00%) and in a third of the surviving
generic-reference gaps is exactly that: upstream models `T[]` as a reference to
the global `Array` interface, and `Promise`, `Object`, `Math`, `Date`, `Symbol`
are among the commonest names that do not resolve. The unresolved-name count
itself has not moved — 10,535 — but the work standing between it and the score
has shrunk. Re-rank lib files from the histogram before the next big slice
rather than from either of these paragraphs.

## Two decisions that shape the rest

### Types are handles, and nothing hands out a reference

Settled by [ADR-0013](../adr/0013-checker-memoisation.md) with three working
implementations before it constrained anything (`crates/tsr-checker-spike`).

Every lazily-computed value in upstream has this shape
(`checker.go:16544`): take a mutable borrow of a side table, recurse into the
checker, then write the result *through the borrow taken before the recursion*.
That is precisely what Rust's borrow checker rejects, and it is not a corner case
— it is how the entire checker computes.

The resolution is that checker methods take `&mut self` and return `TypeId`, and
a type's contents are reached by asking the checker again. Because `TypeId` is
`Copy` and 4 bytes, nothing survives into the recursion:

```rust
if let Some(cached) = self.memo.get(&id) { return *cached; }  // borrow ends here
let resolved = self.compute(id);                              // recursion owns self
self.memo.insert(id, resolved);                               // fresh borrow
```

The measured cost is a second lookup on the miss path. The measured benefit is
17–25% over both interior-mutability styles, *and* no runtime borrow hazard —
the fastest style is also the only one that cannot panic.

### The bit values of `TypeFlags` are load-bearing

Upstream's comment at `types.go:420` is explicit: the numeric values determine
the order `CompareTypes` computes, and therefore **the order of constituents in a
union type**. Union order is printed in every `.types` baseline. So renumbering
these would silently reorder printed unions and fail conformance in a way that
reads like a formatting bug rather than a data-model one. They are ordered by
increasing potential complexity so union processing can bail out early, with
indexed-access and conditional last because those are potentially infinite.

## Distinct types that print the same string

Upstream creates several types that render identically and distinguishes them by
pointer identity:

- `anyType`, `errorType`, `wildcardType`, `blockedStringType`,
  `nonInferrableAnyType`, `autoType`, `intrinsicMarkerType` — all `TypeFlagsAny`.
- `neverType`, `silentNeverType`, `implicitNeverType`, `unreachableNeverType`,
  `uniqueLiteralType` — all `TypeFlagsNever` printing `never`.
- `undefinedType`, `missingType`, `optionalType` — all printing `undefined`.

Merging any pair would be invisible in output and wrong in behaviour:
`errorType` suppresses cascading errors where `anyType` does not, and
`silentNeverType` does the same against `neverType`. `TypeStore::new_intrinsic`
therefore **never interns**, while literal types **always** do — literal identity
is `TypeId` equality, which is what lets a relation check compare `"a"` to `"a"`
without comparing strings.

This is also why every unported expression form yields `errorType` rather than
`anyType`. A gap must never be indistinguishable from a result.

**Correction (2026-08-05):** this paragraph previously said *"both print `any`;
only one of them is a claim that the answer is `any`"*. They do not both print
`any`. `TypeStore::new_intrinsic` gives `errorType` the intrinsic name `error`,
exactly as upstream does (`checker.go:979`), and `type_to_string` prints that
name, so a gap appears in `.types` output as `error`. The consequence is better
than the original claim rather than worse: `examples/types_shapes.rs` separates
"unported" from "wrong" on every one of the 361,683 mismatched lines, and that
distinction is what the ranking above is built on. Upstream's own baselines print
`error` on 1,436 lines, so a coincidental match is possible and rare rather than
impossible.

## Printing is under test, not a convenience

A `.types` baseline compares **whole lines, verbatim**, for every file of a case.
So `type_to_string` is the thing being measured, and one wrong character fails a
case exactly as an outright wrong type does. Two places a port drifts:

- **Numbers.** TypeScript prints a numeric literal *type* as `Number::toString`
  of its value, not as written: `1.0`, `1e0` and `0x1` are all the type `1`.
  Source text cannot be used. Narrowing to `f64` is not a defect but the
  specified behaviour — ECMAScript numbers *are* `f64`.
- **Strings.** Double-quoted with TypeScript's escape table, regardless of how
  the literal was quoted in source.

Both have known gaps, recorded in `bd tsr-4sc.1` rather than papered over.

## The oracle is proved; the producer is not

`checker_types` had no comparison code at all — every case returned
`Unsupported`, so its judging path had executed zero times, which made it exactly
as unproven as a suite reading 100% on its first run. That was closed on
2026-08-05, *before* any checker work was measured with it: `types_suite::compare`
now exists, positional and whole-line, with eight tests each verified against a
deliberately weakened judge. The seven mutations and what each turned red are
tabulated in [checker-oracle.md](checker-oracle.md).

What remains unproven is the **producer**. Nothing renders this crate's types in
`.types` baseline form yet, so `compare` has only ever been fed empty output from
the corpus side. The first non-zero `checker_types` number is therefore still not
evidence until something deliberately wrong has been pushed through the whole
path — checker to renderer to judge — and seen to go red.

## The gradient

`checker_types` now reports two numbers
([ADR-0031](../adr/0031-a-gradient-beside-the-gate.md)). The **case rate** is the
gate and is unchanged: all lines of all files, or the case fails. Beside it is a
**per-assertion-line tally** over the same population — 478,954 lines across the
9,538 judged cases — because a binary gate over 60,269 lines of upstream gives one
bit of feedback per case and would read 0% for months.

The gradient is always the more forgiving of the two, is a strict lower bound (a
missing line costs every line after it, since the comparison is positional), and a
full tally does **not** imply a pass — produce every expected line plus one extra
and `matched == total` while the case is wrong. Never quote it as a pass rate.

The same discipline applies to this crate's own tests, and already caught one of
them: a test asserting the expression memo worked by counting types passed with
the memo deleted, because interning already prevents a repeated literal from
creating a new type. It was replaced with a computation counter, and *that*
version fails when the memo is removed. Every test in
`crates/tsr-checker/tests/types.rs` has been verified to fail against a
corresponding mutation.

## Inherited members, 2026-08-05

`class C extends B {}` now finds `B`'s properties. What landed, and the two
places where it deliberately answers nothing.

### Base types are walked, not flattened

Upstream does **not** put a base class's properties in the derived symbol's
members table. `resolveDeclaredMembers` (`checker.go:19612`) takes exactly
`getMembersOfSymbol(t.symbol)` — the declared members and nothing else — and
`resolveObjectTypeMembers` layers the base types' properties *underneath* them.
Two consequences follow, and both are the reason the obvious shortcut was
rejected:

- a derived declaration **shadows** the base's rather than merging with it, so
  `class B { p: number } class C extends B { p: string }` has two symbols;
- the answer for an *inherited* name is **the base's symbol**, not a copy on the
  derived class.

Flattening the two tables in the binder would have been a few lines and would
have got both wrong — a merge upstream keeps apart, and the wrong symbol identity
for every overridden member. `crates/tsr-checker/tests/members.rs` asserts
through the *declaring line* of the symbol that comes back, precisely because
both shapes print the same type.

### The walk is over symbols, and that is the part to revisit first

Upstream reaches a base through `resolvedBaseTypes`, which are `*Type`s. Here
`TypeData::Named` carries the owning `SymbolId`, and
`getDeclaredTypeOfClassOrInterface` is `new_named_type(symbol, …)` — one type per
symbol with no members of its own. Going type → symbol → base symbol → back is
the same graph with one indirection removed, and it avoids creating a type per
base merely to read its symbol out again.

**How we would know this was wrong:** the moment
`getDeclaredTypeOfClassOrInterface` grows real member resolution — the
*instantiated* members of `class C extends B<number>` — this must move to the
type level, because a symbol has nowhere to hold an instantiated table. That is
the falsifier, and it is `bd tsr-4sc.7`'s remaining work.

### The circularity guard is the path, not a memo

`class A extends B {}` with `class B extends A {}` is a real cycle in the base
graph and the corpus contains such cases deliberately. Upstream guards it in
`resolveBaseTypesOfClass` by parking a `resolvingEmptyArray` sentinel in
`resolvedBaseTypes` and reporting
`Type_0_recursively_references_itself_as_a_base_type` on re-entry. There is no
`resolvedBaseTypes` memo here to park a sentinel in, so the guard is the set of
symbols already on the walk — the same question asked with the state that
exists. It is **not** `Resolutions`: that stack is keyed on
`(symbol, PropertyName)` and is about *type* resolution, and giving base-type
walking a `PropertyName` of its own would change a module this work did not own.
A cycle answers a miss rather than a diagnostic, because the checker has none
(`bd tsr-5e7.6`).

Removing the guard does not fail a test politely: it overflows the stack and
aborts the whole test binary. That is the mutation that pins it.

### A base this port cannot follow makes the whole lookup a miss

Three shapes are gaps: a base with **type arguments** (`extends B<number>` —
nothing instantiates, so `B`'s own `p` would answer `T` where upstream answers
`number`), a base that is **not a plain identifier** (`extends M.B`,
`extends mixin()`), and a name that resolves to something with no members.

The consequential choice is what to do with the *other* bases when one of them is
a gap. Skipping it and answering from the next would be worse than answering
nothing: for `interface I extends A, B<number>` where both declare `p`, upstream
takes `B`'s and we would hand back `A`'s — a wrong symbol, not a missing one. So
one unfollowable base makes the whole lookup a miss. The type's *own* members are
answered before any base is consulted, so this never costs a declared member.

### The meaning passed to base resolution is narrower than upstream's

A class's `extends` names an **expression**, which upstream resolves in value
meaning and reduces through `getBaseConstructorTypeOfClass`; that needs `typeof C`
and construct signatures. This resolves the name in `SymbolFlags::TYPE` meaning
instead — upstream's meaning for the *interface* case, and the one that finds a
class or interface declaration in both. `class C extends someExpression` is
therefore a gap; the declaration form, which is most of the corpus, is not.

### `symbolIsValue` was missing, and it matters more here than upstream

`getPropertyOfObjectType` gates its answer on `symbolIsValue`
(`checker.go:21407`). That gate was not ported, and it is not cosmetic: a class's
or interface's members table also holds its **type parameters**
(`internal/binder/binder.go:429`–`441`), so without it `new C().T` answers with
the type parameter `T`. It is ported here in its value half only; the alias half
needs something that follows aliases (`bd tsr-y4u.12`), so an alias member is a
miss rather than a wrong answer.

This is the one place in this change where a test could not go through a printed
line: `get_type_of_symbol` of a type parameter is `errorType`, which prints `any`
— exactly what a miss prints. The assertion is on the symbol.

### `implements` contributes nothing

Upstream reads only the `extends` clause for base types
(`getEffectiveBaseTypeNode`). An `implements` clause is checked for conformance
and inherits no members. The clause token is the only thing that distinguishes
the two, and dropping that test turns a test red.

## Function, class, enum and module symbols (`bd tsr-4sc.8`)

Ranked first by the histogram at `78cfcba`: 35,488 gap lines — 12.58% of all
gaps — were a symbol kind `getTypeOfSymbol` did not handle (7,485 function
declarations, 7,227 classes, 5,686 methods, 5,569 modules, 2,449 enums), and it
is the only route to the two largest answer buckets still reading **0.00%**:
function/signature at 55,421 lines (11.82% of every aligned line) and `typeof` at
15,912.

### `typeof C` is not a second feature; it is this one's printed form

The most useful thing read out of upstream while porting this, and the thing that
collapses two ranked items into one. `getTypeOfFuncClassEnumModuleWorker`
(`checker.go:16912`) builds **the same type** for all five symbol kinds — an
anonymous object type whose symbol is that symbol. What differs is the *node
builder*:

- `shouldEmitTypeOfSymbol` (`nodebuilderimpl.go:2801`) answers yes for a class,
  an enum or a value module, so `symbolToTypeNode` emits a type **query** and the
  line reads `typeof C`.
- For a function or a method it answers no, because
  `shouldWriteTypeOfFunctionSymbol` (`nodebuilderimpl.go:2760`) requires
  `FlagsUseTypeOfFunction` and the `.types` baseline writer does not set it. So
  the type expands structurally into its call signatures and the line reads
  `(x: string) => void`.

`bd tsr-4sc.10` — the `typeof` **type query node** — remains a separate and much
smaller item. What it is not is the source of the 15,912-line `typeof` bucket.

### What a signature answers exactly, and where it stops

`crates/tsr-checker/src/signatures.rs` builds a [`Signature`] only when every
part of it can be spelled the way the baseline writer spells it. A `.types`
comparison is whole-line, so a signature that is right in three places and
plausible in the fourth fails identically to one that is wrong everywhere — and
is worse than a gap, because `examples/types_shapes.rs` would count it as a
*wrong answer* and mis-rank the work that follows.

| answered exactly | gapped, and why |
|---|---|
| a return **annotation** | an inferred return from a body containing `return` — a subtype-reduced union (`bd tsr-4sc.9`) |
| **no body** → `any` (`checker.go:20016`) | an inferred `async` or generator return — `Promise<T>`, a global (`bd tsr-9or.1`) |
| a **block body with no `return`** → `void` | an inferred return where `mayReturnNever` holds — a function expression, an arrow, an object-literal method: upstream answers `never` or `void` by end-of-body **reachability**, which is the flow graph, built by the binder and read by nothing |
| parameter types, via `getTypeOfSymbol` on the parameter symbol | a **destructuring** parameter — `parameterToParameterDeclarationName` invents a name and the name is compared verbatim |
| `?`, `...`, and defaulted-parameter optionality | any parameter, constraint, default or return type that is itself a gap |
| type parameters with constraint and default | a type parameter carrying `const`/`in`/`out` — the modifiers are read off *every* declaration of its symbol, a merge this port does not do |
| **one** call signature | an overload **set**: two or more signatures print as a type literal `{ (): void; (x: string): void; }` (`nodebuilderimpl.go:2690`), a rendering this slice does not build |

Two of these are worth stating in full because they look like details and decide
thousands of lines.

**A body with no `return` is `void`, and a body that only throws is also `void`.**
`checkAndAggregateReturnExpressionTypes` (`checker.go:20259`) is never-returning
only when `hasReturnOfTypeNever || mayReturnNever(fn)`, and `mayReturnNever`
(`checker.go:20312`) is false for a function declaration and for a class method.
So `function f() { throw 1; }` is `() => void` — which reads like a bug and is
upstream's answer, and is why the reachability question does not arise for the
two commonest kinds.

**A defaulted parameter is optional only from `minArgumentCount` onward.**
`isOptionalParameter` (`utilities.go:303`) returns `parameterIndex >=
getMinArgumentCountEx(signature, …VoidIsNonOptional)`, and with that flag the
function returns the syntactic `minArgumentCount` unchanged. So
`function f(x = 1)` prints `(x?: number) => void` while
`function f(x = 1, y: number)` prints `(x: number, y: number) => void`. Treating
every defaulted parameter as optional is the obvious implementation and is wrong
on the second line. Both spellings were taken from
`conformance/callSignaturesWithParameterInitializers.types` rather than reasoned
about.

### Three divergences accepted, and how each would be shown wrong

1. **The printed name of a type query is unqualified.** A class declared inside
   `namespace M` gets `typeof C`; upstream writes `typeof M.C` at a reference
   site that cannot see `C` directly, and `typeof C` at one that can. This is the
   same divergence already recorded above for named types and has the same single
   cause — the name is computed once at creation and upstream computes it per
   reference site, from `enclosingDeclaration`. It is accepted because a *bare
   identifier* reference is the overwhelmingly common shape and is unqualified in
   upstream too: if you can write `C`, `C` is reachable unqualified from there.
   **Wrong if** the corpus shows a large population of qualified `M.C` lines
   scoring as wrong answers rather than as gaps; today they mostly gap earlier,
   because `M.C` is a property access into a namespace's `exports` and nothing
   reads that table.
2. **`typeof C` carries no members at all.** A class's statics and a namespace's
   exports live in the symbol's `exports` table, and `TypeData::Named` points
   `getPropertyOfType` at `members`. Passing the symbol here would resolve `C.x`
   against the *instance* members — a wrong answer where a gap belongs, which is
   the exact failure the `errorType`-not-`anyType` rule exists to prevent. So
   `C.staticProp` and `M.x` stay gaps. The fix is a second members source on the
   type, not a different symbol, and it belongs with whoever owns `members.rs`.
3. **A class whose base constructor is a type variable prints `typeof C`, where
   upstream prints an intersection.** `getBaseTypeVariableOfClass`
   (`checker.go:16936`) needs `getBaseConstructorTypeOfClass`, which needs
   `checkExpression` on the heritage clause and construct signatures — neither
   exists. **Wrong if** mixin-shaped classes turn out to be a measurable
   population; `class C extends B` for an ordinary `B` is unaffected and is the
   common case.

### A resolution frame upstream does not need, added because this port is eager

Upstream's worker creates an *empty* object type and resolves its signatures only
when asked; the recursion guard lives in `getReturnTypeOfSignature`
(`checker.go:20004`). This port computes a named type's printed form **once, at
creation**, so building a function symbol's type *is* resolving its signature.
The `pushTypeResolution` frame therefore moves from the return type to
`getTypeOfFuncClassEnumModule`. It is a consequence of the eager-printing
divergence rather than an invention, and it is stated in the code at the site.

The other half of that judgement went the other way, and follows the precedent
`declared.rs` set for instantiation depth: **no guard was added around return-type
inference**, because with a `return`-carrying body gapped there is no path from a
function symbol's type back to itself. `typeof f` is unported (`bd tsr-4sc.10`),
a heritage clause is not read, and an annotation reaches only declared types,
which have their own frames. A guard there today would be a guard around a loop
that does not exist.

### "Immediately precedes" had to be re-derived, not transliterated

`getSignaturesOfSymbol` (`checker.go:19806`) drops the *implementation* of an
overload set, so

```text
function fn4a(x?: number, y: string);
function fn4a() { }
```

prints `(x?: number, y: string) => any` — the overload's signature, with `any`
because the overload has no body. Upstream detects the implementation with
`decl.Pos() == previous.End()`, where `Pos()` is the **full** start, the offset
just past the previous token with trivia included; the equality means "with
nothing but trivia between them".

This port's spans start *after* leading trivia
([checker-oracle.md](checker-oracle.md), on the baseline writer's line text), so
the transliterated comparison is false for every overload set written on two
lines — and it was: the test read `error` until it was changed to ask whether the
declaration is the very next child of the shared parent, which answers the same
question from the data this port has. Transliterating a position comparison
across a different span convention is a silent behaviour change, and this one was
caught only because the expected string had been taken from a baseline first.

### One limit is recorded rather than tested

`this` is kept out of `Signature::parameters` and prepended by the printer,
exactly as upstream does — and **no mutation makes that observable**. Folding
`this` into `parameters` shifts `minArgumentCount` by one *and* every value
parameter's index by one, and the only consumer is
`parameterIndex >= minArgumentCount`, so both sides move together and no printed
line changes. The field is kept because it becomes load-bearing with
`getTypeAtPosition` and call-argument checking; it is not covered by a test that
would not bite, on the same reasoning as the two binary-operator guards above.

## Unions, and `boolean` stops being an intrinsic

Ported 2026-08-05 (`bd tsr-4sc.9`), fifth on the re-ranking above: 13,648
assertion lines whose expected answer contains a top-level `|`, reading 0.00%
right, plus three things outside that bucket — `boolean`, the enum divergence,
and the logical operators. Two of the three landed. The third did not, and the
reason is the most useful thing this slice found.

### The order is the answer

A union is compared verbatim, so its constituent order is as much the answer as
its constituent set, and getting it wrong fails a line while *looking* like a
formatting bug. The order is `CompareTypes` (`utilities.go:415`): sort-order
flags first, then the type's name, then per-kind data, then creation order. The
first of those is the numeric value of `TypeFlags`, which this port carried
across one-for-one for exactly this reason and which is now load-bearing rather
than merely documented as such.

`var x: number | string` prints `string | number` — `STRING` is `1 << 5`,
`NUMBER` is `1 << 6` — and upstream records precisely that
(`baselines/reference/submodule/compiler/implicitConstParameters.types:15`).
Every ordering test in `crates/tsr-checker/tests/unions.rs` writes its
constituents in an order the answer does not preserve, because a test whose
fixture happens to be in sorted order proves nothing.

**One prediction was wrong and the implementation was right, again.** `number | "a"`
was expected to print `"a" | number`; it prints `number | "a"`, because
`STRING_LITERAL` is `1 << 10` and sorts *after* `NUMBER`. Upstream's baselines
say `>x : number | "bar"`. That is the eighth time this has happened here, and
the rule holds: check a baseline before "fixing" the code.

### `strictNullChecks` is assumed off, and the assumption is measured

`addTypesToUnion` (`checker.go:25793`) **drops `null` and `undefined`
constituents entirely** when `strictNullChecks` is off. So `string | undefined`
is `string` under upstream's default options and `string | undefined` under
`--strict`, and the difference is not a corner: it touches most unions that
mention either type.

This port has no compiler options — nothing constructs a
`tsr_core::CompilerOptions` and `Checker::new` takes a bound file and nothing
else — so one behaviour had to be chosen.

**Off was chosen**, on three grounds. It is upstream's default. **1,351 of the
corpus's 12,444 cases (10.9%)** set `@strict` or `@strictNullChecks`, so it is
also the majority behaviour by an order of magnitude — measured with a grep over
`_submodules/TypeScript/tests/cases`, not assumed. And it is the assumption the
rest of the crate already makes implicitly: nothing adds `undefined` to an
optional parameter's type either, which is the same option seen from a different
arm.

**The consequence accepted is bad and worth stating plainly:** in a `@strict`
case, every union mentioning `null` or `undefined` is now a *wrong* line rather
than a gap, and `types_shapes.rs` cannot separate those from real defects. The
alternative — gapping any union with a nullable constituent — would have cost
the 89% to protect the instrument on the 11%.

**How this would be shown wrong.** When the checker can read the case's options
(`bd tsr-5s2`, behind the program object `bd tsr-9or.1`) the assumption becomes a
one-line lookup, and the branch is already in the right place. If the corpus
shows the strict cases carry disproportionately many union lines, this should be
revisited before then.

### `boolean` is the union `false | true`, and that is observable

Upstream builds `booleanType` with `getUnionType` at `checker.go:1002`, and it
prints as a keyword because a union of exactly the two boolean literal types
carries `TypeFlagsBoolean`, which the node builder tests *before* it reaches its
union branch (`nodebuilderimpl.go:3255`). This port now does the same.

The point is not the printed form, which was already right. It is that
`boolean` **flattens**: `boolean | true` reduces to `boolean`, because the union
is expanded into its constituents, deduplicated, and put back together. An
intrinsic `boolean` cannot do that, and would have printed `true | boolean`.

The one place the port cannot follow upstream literally is creation order:
`booleanType` is built inside `Intrinsics::create`, where the general path — a
`Checker` method — does not yet exist. `create_boolean_type` constructs it
directly, and the claim that the shortcut is *exact* rather than approximate is
not left to the reader: `boolean_is_the_union_of_the_two_boolean_literal_types`
writes `true | false` in source and asserts the resulting `TypeId` **is**
`intrinsics.boolean`. Interning is what makes that assertion possible, and it is
what makes it worth writing.

**A wrong call, caught by a test.** `formatUnionTypes` (`printer.go:383`) was
first judged unreachable and left unported, on the argument that the keyword
check already handles `boolean`. It does not: the keyword check fires only for a
union of *exactly* two boolean literals, so `string | boolean` printed
`string | false | true`. The function is now ported for its boolean clause,
and the keyword check is no longer consulted for printing at all — it would be a
branch the collapse already covers. Its other two clauses (nullable reordering,
enum collapsing) really are unreachable, for reasons stated in the code rather
than guarded against.

### The enum divergence, replaced

An enum's declared type is now the union of its members' types, carrying
`ENUM_LITERAL` and the enum's symbol, and printing `E` — because the node
builder renders an enum-like type from its **symbol** (`nodebuilderimpl.go:3260`)
rather than from its constituents. That printing rule was read out of upstream
before the code was written, not assumed to fall out.

Printing therefore cannot distinguish the new type from the old one, which is why
`an_enum_declares_a_real_union_that_prints_as_the_enum_name` asserts about the
*type* — `UNION` in the flags, one constituent per member, a symbol — and not
only about the string. Reverting the arm to the old named type turns it red;
nothing about the printed line would have.

**What is left of the divergence, and it is much smaller.** Upstream asks
`getEnumMemberValue` for each member and interns an enum literal type on
(value, enum symbol). This port has no constant evaluator, so every member takes
upstream's *own* fallback for an unevaluable member, `createComputedEnumType`.
The printed form, the constituent count and the per-member identities are all
upstream's; two members that share a value are two types here where upstream has
one. `bd tsr-8pz`.

**One deliberate deviation, confined to one-member enums.** Upstream collapses a
one-constituent union to the constituent, so `enum E { A }` declares the type
`E.A` — which still prints `E`, because the node builder asks
`getDeclaredTypeOfSymbol(parent) == t` and substitutes the enum's name. This port
prints from text computed when a type is created and cannot ask a question whose
answer arrives later, so it keeps the one-element union. The printed line is
right in both positions and the type is one layer thicker than upstream's. When
printing reads the store instead of a cached string, this special case must go.

### Three limits recorded rather than approximated

- **A union containing a *named* union is a gap.** Upstream keeps `E` unexpanded
  inside `E | string` through a denormalised `origin` and `addNamedUnions`
  (`checker.go:25705`). Without it the constituents would be printed —
  `E.A | E.B | string` — which is a wrong line rather than a missing one.
  `bd tsr-ha6`.
- **Named types sort by their printed text, not by their symbol.** Upstream
  compares symbol names and then alias type-argument lists (`utilities.go:608`).
  The two agree for every plain named type and disagree for two references to
  the same generic: `C<string> | C<number>` keeps that order upstream and is
  reversed here. Fixing it means giving `TypeData::Named` a symbol and an
  argument list, which is a reshape of a type two workstreams share.
  `bd tsr-bgz`.
- **No construction limit is ported, and this time that was a real question.**
  Unlike the instantiation case, union construction *is* a recursion. It is a
  recursion over the **source nesting of type nodes**, which is finite in a
  parsed file; the one shape that could be unbounded, a self-referential generic
  alias, terminates because a generic alias's body is never expanded. What
  upstream's depth of 100 and count of 5,000,000 (`checker.go:22111`) actually
  guard is `instantiateType`, which is what makes unions of unions grow. They
  belong there, `bd tsr-el3.2` owns them, and they are not optional then.

### The logical operators are still a gap, and unions were not what they needed

`&&`, `||` and `??` were ranked with this item — 1,860 corpus lines — on the
reasoning that they compute a union of their operands. They do, and unions were
not the blocker.

- `&&` unions `extractDefinitelyFalsyTypes(getBaseTypeOfLiteralType(right))` with
  the right type when `strictNullChecks` is **off**, and
  `extractDefinitelyFalsyTypes(left)` with the right type when it is **on**
  (`checker.go:12495`). Those are different answers, not different spellings of
  one: for `a: string` and `b: number` the first is `number` and the second is
  `"" | number`.
- `||` and `??` reduce with `UnionReductionSubtype`, which is `removeSubtypes`
  (`checker.go:25934`) and needs assignability.
- All three gate on `hasTypeFacts` (`checker.go:30982`), a large table whose
  every arm branches on `strictNullChecks`.

So the operators need compiler options and assignability, and answering them
without either would produce wrong lines rather than missing ones — the one thing
this port has consistently refused to do. They move to `bd tsr-5s2` with the
options work, and the arm in `binary.rs` now says so instead of blaming unions.
