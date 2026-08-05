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

**Superseded 2026-08-05, and the prediction in that last sentence was wrong.**
It was true of the *shape histogram* and never true of the **baselines**:
upstream records both a declaration's declared type and every narrowed
reference, so the population was findable all along without asking this port
anything. It steered the ranking for four cycles on that basis.

Measured by `crates/tsr-conformance/examples/narrowing_cost.rs`, which reads
baselines only — no corpus run, no checker:

```text
3,418 bare-name + 238 dotted = 3,656 lines across 609 baseline files
0.78% of the 468,921 aligned lines
```

An **upper bound twice over**: a line is only lost to narrowing if the declared
type can be computed at all (43.40% today), and the walker is positional, so a
case that fails earlier loses these regardless. Realistically well under a
thousand.

**The filter is the measurement.** Unfiltered — "the reference differs from the
declared type" — reads **24,016**, seven times larger and worthless, dominated
by generic instantiation, names shared across scopes, and property names
colliding with variable names. Three shapes are counted, split at top-level `|`
respecting bracket depth, because a substring test counts nested unions: the
exact error that inflated a union denominator from 17,532 to 30,943 one cycle
earlier. Two independent implementations agree to the line.

**Decision: not built.** 3,656 upper-bound lines do not carry
`getTypeAtFlowNode`, the flow walk, the guard forms, `getTypeFacts`, and real
recursion limits which — unlike the instantiation case — *are* load-bearing.

The honest case for narrowing, which does not show in the count: these are
**wrong answers, not gaps**, the pool the histogram cannot separate from real
defects. That argument carried `bd tsr-tl8` at 21,939 lines. At 3,656 it does
not carry the item, but it is why narrowing stays on the list rather than being
closed.

Unchanged from the previous ranking and repeated only so they are not lost:
`bd tsr-tl8` (the member-name defect, 21,939 lines and the one place this port
can answer *wrongly* where a gap belongs), `bd tsr-4sc.1` (printing — literals
are 87.22% right and 721 of their misses are wrong answers rather than gaps),
and `bd tsr-bb4.1` (per-configuration runs, still worth doing only when the rate
makes the denominator matter).

## The re-ranking, cycle 3 (2026-08-05)

| | cases | gradient |
|---|---:|---:|
| cycle 2 close | 1,076 | 41.27% |
| **cycle 3 close** | **1,163** | **43.40%** |

Landed: calls, function expressions and arrows (`bd tsr-4sc.8`); object literal
expressions (`bd tsr-4sc.9`); the `strictNullChecks` correction; the identity
widening's first two steps (`bd tsr-0e9`); and the wrong-answer attribution
probe. Every guard rail flat to the case.

```text
function/signature   16.48% -> 22.72%
object literal        8.41% -> 24.23%
union                18.90% -> 26.14%
```

### The lib and identity work landed and moved nothing, and that is structural

Stated plainly because it will otherwise be read as a null result. The `.types`
producer gives **each unit its own arena, binds it alone and gives it its own
`Checker`** (`types_producer.rs:273`–`299`). So no lib work and no identity work
can move `checker_types` by a single line until the producer is rewired — that is
a property of the measurement path, not of anyone's commit. The first number any
of it can move is the producer rewire, which is the last step of `bd tsr-0e9`.

This also corrected a ranking claim made here: `bd tsr-6av` (share a bound lib
program across the corpus run) was called a *hard precondition* for `bd tsr-0e9`
paying anything. **It is not** — measured at 15–17 ms per case for the ES5
default set, about three minutes over 12,444 cases and parallelisable
(`crates/tsr-compiler/examples/lib_program_cost.rs`). The reasoning behind the
claim was true at its first two steps — the producer cannot build a program, so
lib work cannot move the number — and its third step converted a fact about
*structure* into a claim about *cost* without measuring the cost. Same shape as
the 10,535 figure two sections below: a true fact extended one inference further
than it licenses, in a direction nobody had reason to question.

### What the histogram now says

Over the 237,199 lines still answered `errorType`:

```text
14,420  CallExpression            — remaining: overload sets and generic callees
13,549  ElementAccessExpression   — nothing ported at all
10,807  a variable initialised by an array literal   (behind bd tsr-0e9)
10,535  a free name that does not resolve, value position
10,404  a property access whose receiver is an identifier we cannot type
 6,978  AsExpression
```

And the **defect** ranking, which no bucket in the shape table can produce and
which only exists because of the attribution probe
([checker-oracle.md](checker-oracle.md)):

```text
6,385  number/string -> any   the implicit any, upstream infers    UNOWNED
1,383  {ours} | undefined     strictNullChecks optionality
1,086  X -> typeof X          getTypeOfNode branch collapse
```

The array bucket is **still 0.00%** at 12,050 gaps, untouched by seven slices,
and entirely behind `bd tsr-0e9`.

## The re-ranking, cycle 2 (2026-08-05, four slices in parallel)

Four items landed together — `getTypeOfFuncClassEnumModule` (`bd tsr-4sc.8`),
unions (`bd tsr-4sc.9`), the type-parameter resolution arm (`bd tsr-y4u.21`) and
lib loading (`bd tsr-9or.1`). Measured at `4533f73`:

| | cases | gradient |
|---|---:|---:|
| start of cycle (`78cfcba`) | 596 | 36.17% |
| inherited members only (`2a0dc19`) | 602 | 36.23% |
| **all four integrated** | **1,076** | **41.27%** |

**+480 cases and +5.10 points.** Every guard rail flat to the case:
`binder_symbols` 8,292/8,459, `printer_round_trip` 11,681/11,737,
`parser_typescript` 5,000/5,031, `file_loader` 96/96.

Three buckets left 0.00% at once, which is what the ranking predicted:

```text
function/signature   0.00% -> 16.48%   (9,133 lines right)
typeof               0.00% -> 52.68%   (8,382)
union                0.00% -> 18.90%   (2,580)
named reference     34.95% -> 40.07%
generic reference   34.97% -> 38.06%
```

### The finding: wrong answers nearly doubled, and that is now a ranked item

The gap pool shrank and the **wrong** pool grew, which no previous cycle saw:

| | before | after |
|---|---:|---:|
| `Identifier`, wrong | 12,558 | **22,360** |
| intrinsic bucket, wrong | 7,603 | **12,964** |
| union bucket, wrong | 1,819 | **3,549** |
| named reference, wrong | 1,740 | **3,340** |

A *gap* is missing work; a **wrong** answer is a defect in what is ported, and it
fails a line exactly as loudly while looking like a result. Roughly 9,800 new
wrong `Identifier` lines arrived with the symbol-typing slice — expected in
direction, since a symbol that previously had no type now has one and can
therefore be *wrong*, but not measured until now. The commonest exact
substitution is `number -> any`, 5,295 lines: we answer the implicit any where
upstream infers a real type.

**This is the first cycle where "fix what is ported" outranks parts of "port more",
and it was invisible until the porting happened.** Same lesson as inherited
members emerging only after property access landed.

### What the histogram says is next

Over the 247,581 lines still answered `errorType`:

| where the checker stops | lines | share |
|---|---:|---:|
| an expression we do not compute | 84,232 | 34.02% |
| an initialiser we do not compute | 43,113 | 17.41% |
| a type node we cannot resolve | 40,004 | 16.16% |
| a property access whose receiver we cannot type | 28,044 | 11.33% |
| a symbol kind `getTypeOfSymbol` does not handle | 16,830 | 6.80% |
| a property access whose property we cannot find | 15,928 | 6.43% |
| a free name that does not resolve, value position | 10,535 | 4.26% |

The individual stops, which is what ranks work:

```text
15,867  CallExpression          — the largest single unported form
13,549  ElementAccessExpression
10,807  a variable initialised by an array literal
10,513  a property access whose receiver is an identifier we cannot type
 6,999  ObjectLiteralExpression
 6,978  AsExpression
 5,596  ArrowFunction
```

And the single largest *substitution* in the whole corpus: **`any -> error`,
50,473 lines** — upstream answers `any` and we gap. Those are unannotated
parameters and error types flowing outward, and they need the signature and
contextual-typing machinery rather than a new node kind. That figure is the
strongest argument yet that the next big slice is calls and signatures rather
than more type nodes.

The array bucket is still **0.00%** (12,050 gaps) and lib attribution still reads
**18,385 lines, 7.43% of gaps** — both now blocked on `bd tsr-0e9`, program-wide
identity, and neither is separately rankable.

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

1. **~~An enum's declared type is a named type, not a union.~~ REPLACED
   2026-08-05 by `bd tsr-4sc.9`.** The record is corrected in place rather than
   deleted, because the shape of the mistake is the useful part: this port built
   a type that printed the enum's name and had none of a union's behaviour, and
   the note said in terms that it must be *replaced* rather than extended. It
   was. `getDeclaredTypeOfEnum` now builds the union of the members' types
   exactly as upstream does (`checker.go:23874`), and it still prints `E` — see
   "Unions, and `boolean` stops being an intrinsic" below for why that printing
   is upstream's rule and not a coincidence. What survives of the divergence is
   much smaller and is recorded there: the member *values* are not evaluated.
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

### `strictNullChecks` is assumed **on** — a correction

**This section replaces an earlier one that said the opposite, and the number in
it was wrong.** The original text is not preserved because it recorded a measured
claim that is simply false; what is preserved, below, is how it was wrong and how
it was caught, which is the part worth keeping.

`addTypesToUnion` (`checker.go:25783`) **drops `null` and `undefined`
constituents entirely** when `strictNullChecks` is off. So `let opt: number | undefined`
is `number` with the option off and `number | undefined` with it on, and the
difference touches most unions that mention either type. This port has no
compiler options, so one behaviour had to be chosen.

**The first choice was "off", on the stated ground that it is upstream's
default. It is not.** `CompilerOptions.GetStrictOptionValue`
(`internal/core/compileroptions.go:294`) reads

```go
if value != TSUnknown { return value == TSTrue }
return options.Strict != TSFalse
```

so an **unset** `strict` yields `strictNullChecks: true`. Recounted over the
corpus: **2,170 of 12,444 cases (17.4%) turn it off explicitly, 1,351 turn it on
explicitly, and 8,923 leave it unset — so it is on for 82.6% of cases.** The
earlier figure, "off for 89%", came from counting only the cases that set
`@strict: true` and assuming the remainder were non-strict. That is the error:
the remainder is not the complement.

Two baselines pin both directions and are what settled it:

```text
predicateSemantics.ts     @strict: false   opt: number | undefined  →  >opt : number
useRegexpGroups.ts        (nothing set)                             →  >result : RegExpExecArray | null
```

**How it was caught, and what that says about the instrument.** It was not caught
by a test — every union test passed under the wrong assumption, because they all
asserted the assumption. It was caught by the *conformance number*: the union
bucket's wrong count went 1,819 → 3,549 on a slice that should only have
converted gaps, and 37.2% of the corpus's 30,943 union baseline lines mention
`null` or `undefined`. A bucket that answers more lines wrong than right is a
signal that the rule is wrong rather than the coverage thin, and it is worth
treating that ratio as a standing alarm.

**The consequence accepted:** a union mentioning `null` or `undefined` in one of
the 2,170 explicitly-non-strict cases is a wrong line rather than a gap — the
same shape of cost as before, one fifth of the size, and pointing the other way.
`bd tsr-5s2` removes the assumption entirely.

### What the correction was actually worth: +0.20 points, and why that is the right size

Measured at `d27db6f`: `checker_types` 1,076 → 1,077 cases, gradient 41.27% →
**41.47%**, about **938 lines**. Set against a wrong-union pool of 3,549 that
looks like a third of the diagnosis, and the gap is worth recording, because
**two of the numbers that framed it were mine and both were too big.**

**The denominator was wrong, the same way the `@strict` count was.** "37.2% of
30,943 union baseline lines mention `null` or `undefined`" came from a substring
test for `" | "` anywhere in the printed type, which counts a nested union inside
a signature or an object type as a union line. Splitting on **top-level** `|`
only, respecting bracket depth: there are **17,532** union assertion lines, not
30,943, and **6,899** of them mention `null` or `undefined` at top level. Same
error class as inferring the strict population from its complement — a quick test
standing in for the real predicate.

**A union line passes only if *every* constituent is right.** Fixing the
nullability of a line whose other constituents are still gaps leaves it exactly
as failing as before. Classifying the non-nullable constituents of those 6,899:

| the rest of the union is | lines | share |
|---|---:|---:|
| intrinsics and literals only | 3,896 | 56.3% |
| …plus a bare name that must resolve | 1,201 | 17.4% |
| a signature, array, object or generic constituent | 1,817 | 26.3% |

So the addressable pool was never 3,549. Take the 3,896, scale by the share of
baseline lines the walker aligns (468,921 of 594,122, 78.9%) — about 3,070 — and
then by the rate at which this checker types the enclosing construct at all
(41.47%): **roughly 1,260 lines realistically reachable.** The measured 938 is
about three quarters of that, which is a good result rather than a shortfall.

**And it cost almost nothing.** The risk this correction accepted was the 2,170
explicitly `@strict: false` cases, where the port now keeps nullable constituents
upstream drops. Counted: **15 lines** across the whole corpus. Small for a
structural reason rather than by luck — upstream drops those constituents in
non-strict cases, so its own baselines have almost no top-level nullable unions
to disagree with.

**The lesson is the object-literal lesson from the other direction.** A bucket
names the answer's shape, not the feature that computes it; and *fixing one
dimension of a line does not make the line pass*. When a correction is worth a
fraction of the pool it addressed, the first thing to check is whether the pool
was measured with the same predicate the score uses.

### Nullable constituents sort first and print last

The correction above made a second rule reachable that had been recorded as
unreachable, and it is the kind that fails lines while looking like a formatting
bug. `UNDEFINED` is `1 << 2` and `NULL` is `1 << 3`, so `CompareTypes` puts both
at the **front** of the constituent list. Upstream prints them at the **end**,
appending `c.nullType` and then `c.undefinedType` after everything else
(`printer.go:407`). `string | undefined` is therefore *stored* `[undefined,
string]` and *printed* `string | undefined`.

Measured over the baselines rather than assumed: **6,811 lines end in
`| undefined`, 28 begin with a nullable constituent, and `null` never follows
`undefined`** — `>d : object | null | undefined`.

`formatUnionTypes` was originally ported for its boolean clause only, with the
nullable clause recorded as unreachable *because nullable types were dropped*.
That justification died with the assumption it rested on. Both clauses are now
ported; the enum clause remains genuinely unreachable (`bd tsr-8pz`).

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

**Amended after the `strictNullChecks` correction above.** With the option known
to default *on*, `&&` takes its second branch, so the option is no longer the
whole blocker for it — `getTypeFacts` and `extractDefinitelyFalsyTypes` are, and
those are real work rather than a lookup. `||` and `??` are unchanged: they still
need subtype reduction and therefore assignability. Do not read `bd tsr-5s2`
landing as automatically closing the operators.

## Calls, function expressions and arrows (`bd tsr-4sc.8`, second slice)

`CallExpression` was the largest single unported form in the corpus — 15,867 gap
lines — with arrows at 5,596 and function expressions at 1,418. They are one item
rather than three: all of them are a **signature** and the anonymous object type
that carries it, reached from three directions.

### The data-model change, and why it was not a field

`getTypeOfFuncClassEnumModule` built a type that could *print* a signature and
could not *answer* one, because the printed string was all it kept. A call needs
the signature back. `TypeData::Anonymous { text, symbol }` is the new variant —
upstream's `newObjectType(ObjectFlagsAnonymous, symbol)` (`checker.go:16925`).

It is deliberately **not** a second field on `TypeData::Named`, because the two
fields point at different tables and one of them must stay unreachable.
`Named.members` is where `getPropertyOfType` looks; for `typeof C` that table has
to be absent, since a class's *instance* members are not `typeof C`'s properties
and answering `C.x` from them would be a wrong symbol rather than an empty
lookup. What a call needs is the symbol's **declarations**, which is where
`getSignaturesOfSymbol` (`checker.go:19806`) reads signatures from. One field
serving both would make one of the two lookups wrong.

The payoff is that a call resolves through the callee's *type*, exactly as
upstream does, so `const h = g; h()` works — a lookup keyed on the callee's
*name* would have gapped it and would have been the easier thing to write.

### Overload resolution is a cliff and was not approached

`resolveCall` (`checker.go:9563`) picks among candidates by assignability, with
inference for generic ones. There is neither here, so **two or more call
signatures is a gap**, and so is a call to a generic signature — its return type
depends on what `T` was inferred as, and answering the uninstantiated `T` would
print a type variable where upstream prints `number`.

Arguments are not checked at all. That is sound rather than a shortcut for the
case that *is* answered: a non-generic signature's return type does not depend on
its arguments, and every diagnostic is `bd tsr-5e7.6`.

### Reachability, replaced by something that refuses to guess

Upstream separates `never` from `void` for a body with no `return` by asking the
flow graph whether the body's end is reachable (`functionHasImplicitReturn`,
`checker.go:20255`). `mayReturnNever` (`checker.go:20312`) means this question
only arises for a function expression, an arrow, or an object-literal method — a
function declaration's empty body is `void` either way, which is why the first
slice could ignore it entirely.

The binder builds the flow graph and nothing reads it. Rather than approximate
it, `block_completes_normally` answers **only where the grammar forces the
answer** and returns "don't know" otherwise:

| statement | answer |
|---|---|
| `throw` | ends the block, and everything after it is dead |
| `if`/`else` where both halves end the block | ends the block |
| `if` with no `else` | completes |
| a declaration, `;`, `debugger` | completes |
| an expression statement **containing no call** | completes |
| an expression statement containing a call | **don't know** |
| a loop, `switch`, `try`, a label | **don't know** |

`() => { throw 1; }` is therefore `() => never` and `() => { let a = 1; }` is
`() => void`, both exactly. **The cost is measured and deliberate:**
`() => { console.log(1); }` is a gap where upstream says `() => void`, because a
call can be typed `never` and this port cannot yet type most calls. The
alternative — assuming a call completes — turns every `never`-returning helper
into a wrong `void`, and a wrong answer is indistinguishable from a result in the
histogram. **How this would be shown wrong:** if the corpus shows the
call-in-body case dominating the arrow population, the rule should become "assume
a call completes unless its type is known to be `never`" once calls to lib
functions resolve.

### The implicit `any` reverses sign here, and that is the sharpest edge in the slice

Everywhere else in this port an unannotated parameter is `anyType` — a *computed*
answer, which is why `get_widened_type_for_variable_like_declaration` uses `any`
and not `error`. Inside a function **expression** that stops being true:

```text
const f: (x: number) => void = x => {};
>x : number
```

Upstream types `x` from the contextual signature. Answering `any` there is a
wrong line wearing the costume of a computed one — the precise failure the
`errorType`-not-`anyType` rule exists to prevent, arriving from the opposite
direction.

So `has_no_contextual_type` must *show* that nothing can supply a contextual type
before an unannotated parameter is answered, and it recognises exactly one shape:
the initialiser of a `var`/`let`/`const` with no type annotation. Every other
position — a call argument, an annotated declaration, an object-literal property,
a `return` expression, an `as` — can supply one, and this refuses to guess which.
The same test guards a literal return: `const f = () => 1` is `() => number`
because nothing supplied a contextual return type, and `const f: () => 1 = …`
is not.

Real contextual typing is the next item here and it is **blocked on function type
nodes**: `getTypeFromTypeNode` has no `FunctionTypeNode` arm, so
`(x: number) => void` in annotation position is `errorType` and there is no
contextual signature to read even where one exists. That arm lives in
`declared.rs`.

### Two guards that no mutation can reach, both kept

Recorded rather than covered by tests that would not bite, on the standard this
document already applies to the `this`-parameter exclusion and the two binary
operator guards.

- **The single-candidate test in `calls.rs`.** An overload set has no printed
  type yet, so the callee is already `errorType` before a call reaches signature
  resolution. The guard becomes the only thing between a call and a guess the
  moment overload sets print as `{ (): void; (x: string): void; }`.
- **`resolve_call_signature` on a non-anonymous type.** An interface with a call
  signature member and a function *type node* both resolve to nothing today, for
  the separate reasons above.

## Object literal expressions, and the two widenings

Ported 2026-08-05, cycle 3. The histogram's largest remaining expression stops
were object and array literals, and the object-literal *answer* bucket read
26,686 lines at 8.41% right with 23,825 gaps. `checker.md` had already predicted
why porting the type node barely moved it — "a bucket names the answer's shape
and not the feature that computes it" — and this collects on that: `{ a: string }`
as a type and `{ a: 1 }` as an expression print identically and are computed by
unrelated functions.

### One renderer, shared, because two would drift

`{ a: string; }` — one space inside each brace, `; ` after every member including
the last, `{}` when empty — is compared character for character. The type-node
path and the expression path now both call `objects::render_object_type`, and
that sharing is under test rather than asserted: replacing the call in
`get_type_from_type_literal` with a second inline renderer turns the *type-node*
test red, which is the only way to prove the two cannot drift apart.

### The two widenings, and why only one of them is here

This is the trap in an object-literal port, and upstream records **two different
types for one source line**
(`baselines/reference/submodule/compiler/widenedTypes1.types:11`):

```text
var c = {x: null};
>c : { x: any; }            ← getWidenedType, at the declaration
>{x: null} : { x: null; }   ← checkObjectLiteral
>x : null
```

- **A member's *literal* type is widened here**, as the literal is checked.
  `checkExpressionForMutableLocation` (`checker.go:13878`) calls
  `getWidenedLiteralLikeTypeForContextualType`, so `const o = { a: 1 }` is
  `{ a: number; }` even though `const n = 1` is `1`. **Freshness stops at the
  property boundary**, and `const` versus `let` makes no difference *inside* a
  literal. This is what a port gets wrong by carrying the initialiser's fresh
  type straight into the member.
- **A member's *nullable* type is widened much later**, by `getWidenedType`
  (`checker.go:18355`), at the declaration.

The second call site is in `symbols.rs`, which this workstream does not own, so
an object literal with a nullable member is a **gap** rather than a line that
would be right as an expression and wrong as a declaration. That guard is not
decoration: removing it turns a gap into a wrong line, and the test that pins it
asserts both directions — nullable gapped, non-nullable still answered.
`bd tsr-mli` owns the call site and the guard's removal together, because doing
either alone is a regression.

### Members keep source order; constituents do not

Worth stating beside the union work, because the two rules are opposite and both
are printed. A union's constituents are sorted by `CompareTypes`; an object
type's members are printed in **declaration order**, because upstream builds a
symbol table and never sorts it. `{ b: "s", a: 1 }` is `{ b: string; a: number; }`.

### What is deliberately not here

Object literal methods (`checkObjectLiteralMethod`, `checker.go:13865`),
shorthand properties, spread properties (`getSpreadType`, `checker.go:13290`),
computed names — which become *index signatures*, and there are none — and
non-identifier string names, which need the printer's quoting rules. Each is a
gap rather than a partial answer, on the rule the type-node path already
established: a partial object type is a wrong answer that looks like a right one.

Two of upstream's three branches in `checkExpressionForMutableLocation` are **not
written** rather than written and left dead: `isConstContext` needs `as const`,
which is a type assertion, and the contextual-type branch needs contextual typing
at all. Both become live together, and `isConstContext` is the one that will need
porting first because it recurses through enclosing literals.

### Array literals are deliberately not in this slice

`createArrayType` is `createTypeFromGenericGlobalType(c.globalArrayType, [elementType])`
(`checker.go:24705`) — an array type **is** a reference to the global `Array`
interface. There are no lib globals until `bd tsr-0e9` lands, so `[1, 2]` cannot
honestly print `number[]`, and a type that merely prints alike is exactly the
divergence this document has now deleted twice.

Two things for whoever takes them: the printed form `number[]` is a **node
builder special case** keyed on `sym == b.ch.globalArrayType.symbol`
(`nodebuilderimpl.go:3370`) and not a property of the type; and
`checkArrayLiteral` (`checker.go:8021`) is dominated by contextual typing —
tuple context, spread elements, const context — so computing the element type is
not the hard part.

## Type assertions, and the two rules behind one node kind

Ported 2026-08-05, cycle 4. `AsExpression` was 6,978 gap lines and
`TypeAssertionExpression` 712 — the largest remaining item needing no
assignability, no inference and no lib globals.

An assertion looks like one construct and is two, and they are opposites:

- **`x as T` discards the operand's type entirely** and answers
  `getTypeFromTypeNode(T)`. The operand is checked only so its own baseline
  lines exist and so the assignability diagnostic can be reported — upstream
  defers even that (`checkAssertionDeferred`, `checker.go:12315`). The
  consequence is that an assertion **pays where nothing else does**:
  `unknownThing as string` is `string`, because a gap in the operand cannot
  reach the answer.
- **`x as const` has no type to resolve at all** and answers
  `getRegularTypeOfLiteralType` of the *operand's* type.

So `const` has to be recognised **before** the type node is resolved. Upstream
makes the same point from the other side: `resolveName` has a special case so
that the `const` in a const assertion is never resolved (`utilities.go:134`).

### The `const` arm is ported and unreachable, and the cause is in the parser

`crates/tsr-parser/src/types.rs:981` parses a type reference's entity name with
`parse_identifier`, where upstream uses
`parseEntityName(allowReservedWords: true)` (`parser.go:2897`). So `x as const`
parses to a `TypeReferenceNode` whose `type_name` is `None` — **and no
diagnostic is reported** — and `isConstTypeReference` can never match. All 257
const assertions in the corpus are gaps, and so is any type reference with a
keyword segment such as `X.default`. `bd tsr-0ao`.

Mutating the order of the two arms turns **no test red**, and that is recorded in
the module rather than covered by a test that could not bite. The arm is kept
because it is upstream's behaviour and becomes load-bearing the instant the
parser is fixed. Its tests exist and are `#[ignore]`d naming the issue, so they
turn green on their own rather than needing to be remembered — and they are
deliberately **not** rewritten to assert today's `errorType`, which would pin the
inferior answer.

**This is the better form of the "currently unobservable" note.** This document
carries several such notes, and the `strictNullChecks` correction showed how they
fail: a guard correctly marked "cannot fire today" becomes load-bearing when an
unrelated fix lands, and nothing flags it. A note with a **named unblocking
event** and an `#[ignore]`d test attached does flag it, because the test starts
passing. The existing notes — the symbol-flags dispatch in `getTypeOfSymbol`, the
destructuring check in `binary.rs`, the order of `+`'s numeric and string tests —
have no such attachment and should acquire one.

### A const assertion on an object literal would be wrong twice over

`{ a: 1 } as const` is `{ readonly a: 1; }`
(`baselines/reference/submodule/conformance/es2020IntlAPIs.types:188`), and this
port would answer `{ a: number; }`. Two independent things are missing, and only
one is obvious:

1. **The members are `readonly` and unwidened**, and that does not happen in
   `checkAssertion` at all — it happens inside `checkObjectLiteral`, which asks
   `isConstContext` (`checker.go:13615`) and, when true, takes the regular type
   instead of the widened one and sets `CheckFlagsReadonly`. `isConstContext`
   recurses through enclosing parentheses, array literals, spreads and property
   assignments, so an assertion many levels up still reaches every member.
2. **A string member would print with the wrong quotes.** A string literal type
   prints double-quoted standing alone and **preserves the source's quote style
   inside an object type**:

```text
const options1 = { localeMatcher: 'lookup' } as const;
>options1 : { readonly localeMatcher: 'lookup'; }
>'lookup' : "lookup"
```

The same type, two spellings, because the node builder reuses the source type
node for the member. This port normalises every string literal to double quotes,
so it would fail the line even with `isConstContext` ported.

Porting only the first would give right answers for numbers and wrong lines for
every string — worse than a gap and much harder to spot. `bd tsr-7ja` owns both
halves together. An **array literal** operand needs no guard: array literals are
unported, so the operand's type is already `errorType` and it propagates.

## Element access, and one interaction with optionality (`bd tsr-4sc.8`, third slice)

13,549 gap lines — the largest single unported form left once calls landed.

### `a["b"]` is a property access, and upstream says so

The whole slice is one observation. `getPropertyNameFromIndex` (`checker.go:21786`)
derives a property **name from the index's *type***, and
`getPropertyTypeForIndexType` then calls the same `getPropertyOfType` that
property access calls. So an element access with a literal index is not a second
kind of lookup; `crates/tsr-checker/src/indexed.rs` is that observation plus the
cases where no name can be derived.

Taking the name from the index's *type* rather than its *syntax* is upstream's
choice and it pays immediately:

```text
const k = "b";
a[k]        // k's type is the literal "b", so this resolves
```

A syntactic reading sees an identifier and gaps. Because a `const` initialised
with a string literal keeps its literal type — the freshness rule this crate
already had — the type-directed reading answers it. `let k = "b"` widens to
`string`, names no property, and is a gap: the same rule read from the other
side, and the assertion that distinguishes the two implementations.

Gaps, each named: a non-literal index (needs index signatures, which no type here
has), an optional chain, a `unique symbol` index, and a name that is not a
property of the receiver — including arrays and tuples, whose members live in
`lib.d.ts` (`bd tsr-9or.1`).

### A signature prints a parameter's annotation, not the parameter's type

Found by two existing tests going red when `crate::optionality` landed, and it is
a genuine defect in the *printer* that was invisible until then.

Upstream records **both** spellings of the same optional parameter
(`compiler/assertionWithNoArgument.types`, a `@strict: true` case):

```text
export function assertWeird(value?: string): asserts value {
>assertWeird : (value?: string) => asserts value
>value : string | undefined
```

The declaration line prints `getTypeOfSymbol`, which carries the `| undefined` a
`?` adds. The signature prints the annotation **as written**, because
`symbolToParameterDeclaration` hands the type to `serializeTypeForDeclaration`
(`nodebuilderimpl.go:2216`), which reuses the written type node rather than
re-printing the computed type.

This port took `getTypeOfSymbol` for both. That was accidentally right while an
optional parameter's type was just `number`, and became wrong the moment
optionality was ported — printing `(value?: string | undefined)`, a spelling that
appears nowhere in the corpus. `parameter_of` now prefers the annotation.

**This is a printer fix, not an argument against optionality.** The baseline
above is the evidence that adding `| undefined` to the *symbol's* type is right;
what was wrong was reusing that type in a position where upstream reuses the
syntax. The two questions were conflated because, before, they had the same
answer.

Corpus counts, for whoever revisits the `strictNullChecks` assumption: `?: X`
appears 10,073 times in signature position against 958 for `?: X | undefined`,
and the second set is `@strict: true` cases. After this fix the signature half is
insensitive to the assumption, so what remains to decide is only the declaration
line.

## Narrowing, measured at last: 3,656 lines, and the prediction retires

`bd tsr-4sc.11` has been ranked on a prediction for four cycles. This document
said narrowing "adds no bucket of its own and cannot be seen by this histogram;
it becomes measurable once items 1 and 2 land". Both landed, so the prediction
came due — and the second half of it turns out to have been wrong in a useful
way. Narrowing is invisible to the *shape* histogram and it was never invisible
to the **baselines**.

### The instrument reads baselines only

`crates/tsr-conformance/examples/narrowing_cost.rs`. No corpus run, no checker,
no contention with anyone else's measurement — seconds to re-run. Upstream's own
`.types` baseline records both halves of every narrowing:

```text
var strOrBool: string | boolean;
>strOrBool : string | boolean      ← the declared type
if (typeof strOrBool === "boolean") {
    bool = strOrBool;
>strOrBool : boolean               ← narrowed
```

For each name, the **first** assertion in a file section is its declared type;
any later assertion that is a strict narrowing of it is a line this port answers
**wrongly** today, because `checkIdentifier` is ported only as far as "resolve
the name, take the symbol's type".

### The number

```text
                                     bare name  a.b / this.x
union constituent(s)                      2385           229
literal of the primitive                   871             8
literals of the constituents               162             1
total                                     3418           238
```

**3,656 lines across 609 baseline files — 0.78% of the 468,921 aligned lines**,
and that is an *upper bound* twice over: a line is only lost to narrowing if this
port can compute the declared type at all (43.40% today), and the walker's
comparison is positional, so a case already failing earlier loses these lines
regardless. The realistic figure is **well under a thousand**.

### The filter is the measurement

An unfiltered "the reference's type differs from the declared type" count reads
**24,016** — nearly seven times larger and worthless. Inspecting the residue
shows why: generic instantiation (`f` declared `(x: "foo") => "foo"`, referenced
as `(x: T) => T`), two different symbols sharing a name across scopes, and
property names colliding with variable names. Only three shapes are counted: a
proper subset of the declared union's constituents, a literal of the declared
primitive, and constituents that are literals of the declared ones. Splitting is
at **top-level** `|` respecting bracket depth, because a substring test counts
nested unions — the same error that inflated a union denominator from 17,532 to
30,943 one cycle earlier.

The two implementations were written independently — a throwaway script and the
committed example — and agree to the line, which is the only reason to trust a
number produced by a filter this aggressive.

### Correction, same day: the total is ~7,800, and the decision reverses

**The measurement below was right about the population it measured and wrong
about the item.** Recorded in place, because the mistake is instructive and the
conclusion it produced was acted on.

Binder bucketed the implicit-any population independently and found its largest
bucket is **not** an inference gap: 4,140 lines (45.89%) are a *reference to an
implicitly-any variable*. `let x; x = 1; x` answers `number` at the use site,
because an implicitly-any variable gets **`autoType`** (`checker.go:976`) — a
distinct intrinsic that prints `any` — and `checkIdentifier` routes auto-typed
references through flow analysis (`checker.go:11133`, `:11182`). That is
`getFlowTypeOfReference`. It is the same item.

**The two sets are disjoint, measured rather than assumed: the overlap is 0.**
It is disjoint *by construction*, which is why it could be checked cheaply — a
narrowing needs a declared type to narrow **from**, a union or a primitive with
literals, and `any` is neither. `narrowing_cost.rs` now reports both buckets and
their intersection instead of one bucket alone.

```text
narrowing of a declared type   3,656   (this measurement, baseline ceiling)
`any`-evolution                4,140   (binder, measured against our answers)
overlap                            0
                             ≈ 7,800
```

**So narrowing is worth roughly 7,800 lines, not 3,656, and the "not built"
decision below is superseded.**

**The lesson is about the instrument, not the arithmetic.** A filter tight enough
to be trustworthy also hides everything just outside it, and nothing warns you.
This one was built to answer "what does a *declared* type failing to narrow
cost", answered exactly that, and could not see the adjacent failure mode where
the declared type is `any` and the mechanism is evolution rather than narrowing.
Two people measuring two populations both correctly is how an item ends up
under-ranked by more than half.

### A prerequisite neither measurement showed: this port has no `autoType`

`get_widened_type_for_variable_like_declaration` returns `intrinsics.any` for a
declaration with no annotation and no initialiser. Upstream returns `autoType`,
which is a **different type that prints the same string** — the trap this
document already catalogues for `errorType`/`anyType` and `neverType`/
`silentNeverType`, and the one place the port has fallen into it.

It is not cosmetic. `t == c.autoType` is the *entire* test by which
`checkIdentifier` decides to do flow analysis at all, and an explicit `: any`
must **not** evolve. So none of the 4,140 can be addressed until `autoType`
exists and that arm returns it. Cheap, separable, and a strict prerequisite.

### The decision: not built

**Superseded by the correction above — read that first.** The reasoning below
stands on its own numbers and is kept because the effort argument is unchanged;
only the line count it was weighed against has moved.

**Narrowing is not worth building yet, and this is the record that retires the
ranking rather than deferring it again.** 3,656 upper-bound lines against a build
that needs `getTypeAtFlowNode`, the flow-graph walk, the guard forms, and
`getTypeFacts` — the same large table the logical operators are blocked on — plus
real recursion limits (`bd tsr-el3.2`), which unlike the instantiation case are
load-bearing here because the recursion is genuine.

For comparison, intersections are 1,082 gap lines, sit entirely in `declared.rs`,
and reuse the ordering, interning and printing machinery unions already built.
Fewer lines, a fraction of the work.

**One thing that does not show in the line count, and is the honest case for
narrowing:** these 3,656 are *wrong answers*, not gaps. They are in the pool the
histogram cannot separate from real defects, which is the same argument that made
`bd tsr-tl8` worth doing at 21,939 lines. At 3,656 it does not carry the item on
its own, but it is why narrowing should not fall off the list entirely.

**How this would be shown wrong.** The bound assumes a narrowed reference is
always a *strict* narrowing of the declared type as printed. Narrowing that
produces a type not expressible as a subset of the declared constituents — an
intersection from a `this`-based guard, or `NonNullable<T>` on a type parameter —
is invisible to this filter. If someone finds that population is large, re-run
with a looser filter and a manual audit of the residue.

## Intersections: source order, and the reduction that needs no assignability

Ported 2026-08-05, cycle 5, on the strength of the narrowing measurement above:
1,082 gap lines against narrowing's 3,656 upper-bound, but entirely inside
`declared.rs` and reusing what unions already built.

### The rule that is *not* symmetric with unions

A union's constituents are sorted by `CompareTypes`. An intersection's are held
in an `orderedSet` (`checker.go:26057`) and **kept in the order they were
written**. `var x: M1 & C1` prints `M1 & C1`; `var x: C1 & M1` prints `C1 & M1`.
Both spellings appear in the baselines, which is exactly what a sort would make
impossible — and taking it by symmetry from the union work is the mistake this
slice was most likely to make.

A **union constituent is parenthesised**: `T & ({} | null)`. Nothing else in the
corpus needs parentheses inside an intersection — not one baseline line has a
function type as an intersection constituent — so only that case is ported. A
general precedence table would be a guess everywhere it was not exercised.

The two orders coexist in one printed type, and one test pins that:
`A & (number | string)` prints `A & (string | number)` — source order outside,
sorted order inside.

### Most of the emptiness rules need no assignability at all

This was the surprise, and it is why the item was cheap. Upstream's intersection
reductions are largely **pure flag arithmetic** over `TypeFlagsDisjointDomains`
(`types.go:480`): a type from one domain beside a type from any other is the
empty set. `string & number` is `never`, and the baselines record exactly that
(`switchCaseWithIntersectionType`). Two distinct *unit* types reduce the same
way, via upstream's own trick of setting `NON_PRIMITIVE` in `includes` so the
disjoint test fires (`checker.go:26283`) — so `"a" & "b"` is `never` while
`"a" & "a"` is `"a"`.

`removeRedundantSupertypes` is the mirror of the union rule and equally free:
a union drops the *literal* beside its primitive (`string | "a"` is `string`),
an intersection drops the *primitive* beside its literal (`string & "a"` is
`"a"`).

Note that `OBJECT` is **not** one of the disjoint domains, which is why
`A & string` survives where `object & string` does not.

### Two gaps, both because the alternative is right only sometimes

- **A two-constituent intersection of a type variable and a primitive**
  (`T & string`). Upstream asks `getBaseConstraintOfType` and
  `isTypeStrictSubtypeOf` (`checker.go:26128`) and may answer `T`, `never`, or
  the intersection itself depending on `T`'s constraint. There is no way to pick
  without assignability, and the plain intersection would be right *sometimes* —
  which is worse than a gap, because it cannot be found again.
- **An empty object constituent** (`A & {}`). Upstream tracks
  `IncludesEmptyObject` and gives it rules of its own: `X & {}` deliberately
  skips supertype reduction, and `{}` is removed beside a definitely-non-nullable
  type. Neither is ported. It is recognised here by its *printed form* rather
  than by an object flag, since this port has no `ObjectFlags` — a shortcut that
  is only safe because the answer is a gap either way.

`silentNeverType` is upstream's other answer where `never` is returned; this port
does not create one, and that is the usual "distinct types that print the same
string" note rather than a new divergence.

### `TypeData::Intersection` is a separate variant from `TypeData::Union`

Despite the identical shape. They differ in the one respect that is printed —
sorted versus source order — and merging them behind a flag would put that
distinction one indirection away from the code that has to respect it. Interning
shares the one table, safely, for the reason already recorded for unions: the
`TypeData` discriminant is part of the derived `Hash` and `PartialEq`.

## Array types are a reference to the global `Array`

Ported 2026-08-05, cycle 7. The array bucket was **12,491 lines at 0.00% right
through eight slices** — the largest bucket that had never scored — and it was
blocked on the global `Array` not existing. It exists as of `fa16ae4`, and the
only thing left was that `getTypeFromTypeNode` had no `ArrayType` arm.

### Not a type that prints `T[]`

This is the whole point, and `checker.md` had already warned against the
alternative: this port should not answer arrays "with another type that merely
prints alike". `getArrayOrTupleTargetType` (`checker.go:24148`) picks
`globalArrayType` and `createTypeReference` instantiates it, so an array type
**is** a generic reference — interned on the same `(target, arguments)` key the
generic-reference machinery already used.

The consequence is testable and is tested: **`string[]` and `Array<string>` are
one type**, not two that print alike. A lookalike implementation passes every
printing test and fails that one.

### The shorthand belongs to the target, not to the syntax

`typeReferenceToTypeNode` (`nodebuilderimpl.go:2977`) special-cases
`globalArrayType` before anything else, so **`Array<Base>` prints `Base[]`** —
upstream records exactly that (`generatedContextualTyping`). Putting the
shorthand in the array *node* would have printed `Array<Base>` for the long
spelling and passed every fixture written with `[]`.

`readonly T[]` is a **different global** (`globalReadonlyArrayType`), not a
modifier: two distinct types that share an element. The `readonly` type operator
node is itself transparent (`checker.go:22973`), which is what lets the array
node see it as a parent and pick the other target.

### Parenthesisation came from baselines, not from a precedence table

Only some of the plausible cases are wrapped, and guessing would have been wrong
in both directions:

```text
(string | number)[]     (typeof Alpha)[]      (() => string)[]
{ (x: number): number; }[]      string[][]    string[]
```

A union, a `typeof` and a signature are wrapped; an **object type and a nested
array are not**. Intersections are wrapped on the same precedence grounds and
**no baseline exercises one** — stated rather than presented as verified.

### Tuples are a gap, deliberately

Upstream reaches them through the same function, with `globalTupleType` and a
per-element flags model (`optional`, `rest`, `variadic`). Answering `string[]`
for `[string, number]` would be a wrong line dressed as a right one, and the
11,784 lines are in the array half. `bd tsr-cqi`.

### An unobservable guard, made observable instead of documented

The arity check in `global_type_symbol` — upstream's `getGlobalType("Array", 1)`
— turned **no test red** under mutation, because every fixture supplied an
`Array` of the right arity. Rather than record it as unobservable, which is what
this document has done four times, the fixture that reaches it was written: a
program declaring `interface Array {}` must **not** have its zero-arity `Array`
used as the array target.

That is the better resolution wherever it is available. The `#[ignore]`d test
with a named unblocking event is for guards that genuinely cannot be reached
yet; a guard that can be reached by a fixture nobody had written is not one of
them.

## Index signatures, and the half of them that is blocked (`bd tsr-4sc.8`)

`ElementAccessExpression` was still the second-largest single stop at 13,442
lines after the literal-index slice — the half deliberately gapped, because a
non-literal index needs index signatures and no type had any.

### The applicability rule is asymmetric, and it was taken from upstream

`isApplicableIndexType` (`checker.go:19040`) is the whole feature in one
function, and it does not read the way symmetry would suggest:

- a **string** index signature applies to a `string` key **and to a `number`
  key** — `{ [k: string]: T }` answers `a[0]`;
- a **number** index signature applies only to a `number` key, plus a string
  literal that *spells* a number (`isNumericLiteralName`), never to `string`.

`findApplicableIndexInfo` then adds a precedence rule in its own comment —
*"index signatures for type `string` are considered only when no other index
signature applies"* — so a type carrying both answers a numeric access from the
**number** signature.

Getting either backwards looks right on half the corpus, which is why all three
are pinned by tests and each has a mutation that turns one red.

**Applicability is decided structurally, not by assignability.** Upstream asks
`isTypeAssignableTo` and there is no relation here. For the key shapes this port
can produce — the `string` and `number` intrinsics and their literal types —
assignability is decidable by inspection; every other key is a gap rather than a
guess, because a wrong index type yields a confident wrong *value* type.

`isNumericLiteralName`'s round-trip is the definition rather than a shortcut:
`"0"` is a numeric name, `"00"`, `"1.0"` and `" 1"` are not, and all four are
distinct property names.

### It pays on interfaces only, and the blocker is one function away

Measured rather than assumed: every case works through an `interface` and
**none** works through a type literal. `get_type_from_type_literal`
(`declared.rs`) gaps the *whole* type when it meets a member it cannot render,
and an index signature is such a member — so `{ [k: string]: number }` is
`errorType` before any lookup happens.

That is the right rule for a printer (a partial object type is a wrong answer
dressed as a right one) and it means this slice reaches only the named half of
its population. Teaching `get_type_from_type_literal` to render
`{ [k: string]: number; }` unblocks the rest, and it is a change in a file this
cycle did not own.

### What is not ported

- **`noUncheckedIndexedAccess`.** An index signature does not make a property
  optional and does not add `| undefined`. That is a compiler option this port
  does not read (`bd tsr-y5a`), and the two are deliberately not blended.
- **Inherited index signatures.** `resolveObjectTypeMembers` layers a base's
  under the derived type's; this reads a symbol's own declarations only, so an
  inherited one is a miss. A gap, not a wrong answer, and it belongs with base
  type walking.
- **Merging several applicable signatures** into a synthetic `IndexInfo` over the
  intersection of their value types. Two applicable signatures is a gap.
- Index signatures on a class or a mapped type, and `symbol`/pattern keys.

## Array literals: two pieces of machinery meeting

Ported 2026-08-05, cycle 8. 15,053 gap lines — 10,807 declarations whose
initialiser is an array literal, 4,246 the literals themselves — and the largest
buildable population on the board, unblocked by the array *type* landing one
slice earlier.

Almost nothing here is new. The element type is the **union of the elements**,
and the result is an **array type**, so `checkArrayLiteral` is `crate::unions`
meeting `crate::declared`: `[1, "a"]` is `(string | number)[]` because the union
sorts by `TypeFlags` and the `Array` reference prints its element parenthesised.
Upstream records that exact line.

The payoff of the array type being a real reference rather than a lookalike
shows up here and is tested: **`[1]` and `number[]` are one type**, the inferred
and the written form interned together.

### `[]` is `never[]`

`implicitNeverType` under `strictNullChecks`, `undefinedWideningType` without it
(`checker.go:8098`). The corpus splits **461 `never[]` to 297 `undefined[]`** on
exactly that option — an independent confirmation of the assumption this crate
made three slices ago, arrived at from a different direction. It is also the
cheapest test that separates a real implementation from one that only handles
the non-empty path.

`implicitNeverType` is another of upstream's distinct types printing `never`, and
this port has only `neverType`; recorded rather than merged silently.

### The wrong union reduction, deliberately, and where it shows

Upstream reduces the element union with **`UnionReductionSubtype`**
(`checker.go:8096`); this port has only `UnionReductionLiteral`, because
`removeSubtypes` needs assignability. For every element type this slice can
produce the two agree — widened primitives are mutually unrelated, and literal
types survive only in a const context, which is unported.

They part company on **object-typed elements**, and not subtly: an object literal
type is not interned, so `[{a: 1}, {a: 1}]` is a union of two *distinct* types
that print the same string, which subtype reduction collapses to one. Printing
`({ a: number; } | { a: number; })[]` would be a wrong line that reads as a
formatting bug. So two or more object-typed constituents make the literal a gap,
and **one does not** — `[{a: 1}, 1]` is `(number | { a: number; })[]`, which
subtype reduction would not have merged either.

That element order was predicted wrong and the implementation was right for the
tenth time: `NUMBER` is `1 << 6` and `OBJECT` is `1 << 20`, so the object type
sorts second.

### A guard that is unobservable today and load-bearing under one named edit

Spreads and omissions are rejected before the element is checked. Deleting that
test changes nothing **today**, because `check_expression` has no `SpreadElement`
arm and the gap-in-an-element guard catches them anyway.

Rather than record that and move on — which this document has done four times —
the pair of mutations that makes it matter was applied together: remove the guard
*and* give `SpreadElement` its operand's type, which is the obvious next edit.
`[...[1]]` then answers `number[][]` and the test goes red. So the guard is kept
with its unblocking edit named, and the test is known to bite rather than assumed
to.

This is the third resolution for an unobservable guard, and they now form a
preference order: **make it observable with a fixture** (the `Array` arity check);
failing that, **verify it against the named future edit** (this, and the tuple
gap); failing that, an `#[ignore]`d test naming the issue (the `const` assertion
arm). Only when none of the three is possible should it be prose.

## Signature members print with a colon, not an arrow

Ported 2026-08-05, cycle 9. `get_type_from_type_literal` rejected the **whole**
literal unless every member was a property signature, so `{ m(): void }` never
reached any dispatch — 3,337 gap lines, and none of them touchable by the
function-type work they had been counted under.

### The spelling is the slice

A **method member** prints `m(): void`; a property holding a function type
prints `m: () => void`. Same signature, two forms, chosen by position — upstream
emits a `MethodSignature` in the first case and a `FunctionTypeNode` in the
second, and the difference is a colon against an arrow.

So `signature_member_text` deliberately does **not** reuse `signature_to_string`,
which renders the arrow form. Converting one into the other by string surgery
would have to find the top-level `) => `, and a parameter type can contain one.
The two functions render two different things and share nothing, which is also
upstream's arrangement.

### `Member` became two shapes rather than one with blank fields

(Three, since index signature members — see below.)

A call signature has **no name at all**, and a method's text is whole rather than
`name` + `: ` + `type`. Modelling either as a property with an empty name would
move the distinction from the data into the renderer, where the two callers of
`render_object_type` could then drift. `Member::Property` and `Member::Signature`
keep it in one place, and the object-literal path uses the same enum.

### The reject-the-whole-literal rule is unchanged

This slice raises what is renderable; it does not lower the bar. An accessor is
still unported and a literal containing one is still a gap — including a method
whose parameter type is a gap. "A partial object type is a wrong answer that
looks like a right one" still holds, and a mutation that lets an unbuildable
method fall through turns the test red.

### Call and construct signatures gap on someone else's file

`signature_parts_of` (`signatures.rs`) has arms for `FunctionDeclaration`,
`FunctionTypeNode`, `MethodDeclaration` and `MethodSignatureDeclaration` — and
none for `CallSignatureDeclaration` or `ConstructSignatureDeclaration`. So
`{ (): number }` gaps on that and nothing else: the dispatch recognises it and
the rendering is in place, both exercised by the method cases.

Two match arms in `signatures.rs` make them live. That file is another
workstream's, so this is `bd tsr-qk9` with an `#[ignore]`d test naming it —
resolution #3 of the three-way preference order recorded above, used here
because the first two are unavailable: no fixture reaches the arm, and the
"named future edit" is in a file this workstream must not touch.

## Index signature members, and the discovery that member order is not source order

Ported 2026-08-05. Index signatures themselves landed earlier (`8f7f3cc`) but
reached only types declared through an `interface`, which prints by **name** and
so never renders its members. A type literal has no name, so
`get_type_from_type_literal` had to render the member — and until this slice it
gapped the whole literal on any member it could not render. `{ [k: string]: number }`
was therefore `errorType` before any `a[i]` lookup could happen.

### `Member` became three shapes

An index signature is neither of the two above: it has no property name (the
identifier inside the brackets is a *parameter* name, which is why upstream keeps
it on the declaration and not on the `IndexInfo`), and it carries a key type
where a property carries nothing. `Member::Index { readonly, name, key, value }`,
for the same reason `Member::Signature` exists — the distinction belongs in the
data, not in the renderer that two call sites share.

The parameter name is printed **as written**. The baselines are unambiguous:
`{ [key: string]: string; }` (52 lines) and `{ [x: string]: unknown; }` both
occur, so there is no canonical spelling to normalise to.

### Upstream does not print members in source order — and this port did

The finding that made this slice bigger than one match arm.
`createTypeNodesFromResolvedType` (`nodebuilderimpl.go:2627`) emits **call
signatures, then construct signatures, then index infos, then properties**, off
a `StructuredType` whose four collections were separated when the members were
resolved. Source order is not preserved across the group boundaries:

```text
declare const myRecord2: { a: string; b: string, [key: string]: string }
>myRecord2 : { [key: string]: string; a: string; b: string; }
```
(`baselines/reference/submodule/conformance/noUncheckedIndexedAccess.types:377`)

`get_type_from_type_literal` had accumulated members in one source-order vector
since it was written. That was invisible while properties were the only member
kind and stayed invisible after signature members landed, because a
property-and-method literal *is* in one group. An index signature is the first
member kind that makes the difference observable, so the ordering defect and its
fix belong to this slice rather than to `4bf4b8c`.

The grouping is applied in `get_type_from_type_literal`, where the member kind is
known, and **not** in `render_object_type`, which emits its slice verbatim. The
reason is that the enum cannot recover the grouping on its own: a **method** is a
`Member::Signature` here but a *property* upstream —
`addPropertyToElementList` renders it from the property symbol — so it groups
after the index signatures, while a call signature groups before them. Two
members with the same variant, two different groups.

### The renderable set widened; the bar did not move

An index signature whose key is neither the `string` nor the `number` intrinsic
still gaps the whole literal, and deliberately takes the **same** gaps as
`index_info_of` on the lookup side. `[k: string | number]` is upstream *two*
index infos (`getIndexInfosOfIndexSymbol` splits it), not one printed with a
union key, so printing it whole would be a confident wrong answer; and if the
printer and the lookup disagreed, a literal could print a signature that a
subsequent `a[i]` then fails to find.

Merging several applicable signatures over an intersection remains unported, as
recorded in `index_signatures.rs`. Inherited index signatures landed next — see
below.

## Inherited index signatures, and a decoration caught by running the mutation

Ported 2026-08-05, immediately after the above. `index_infos_of_symbol` did not
follow base types, so `interface D extends B {}` found none of `B`'s index
signatures. The walk `get_property_of_declared_symbol` already does for
inherited *properties* was extended rather than duplicated;
`base_symbols_of` became `pub(crate)`.

### The rule is a shadow by key type, not a merge

`resolveObjectTypeMembers` (`checker.go:19149`):

```go
indexInfos = core.Concatenate(indexInfos, core.Filter(inheritedIndexInfos,
    func(info *IndexInfo) bool { return findIndexInfo(indexInfos, info.keyType) == nil }))
```

A derived `[k: string]: A` hides a base's `[k: string]: B` **outright** — the two
are never combined — while a base's `[k: number]` survives beside it.

### `None` is a gap; `Some(vec![])` is "none declared"

`get_index_infos_of_type` returns `Option`. An unfollowable base (type
arguments, a qualified name) or a cycle yields `None`, because a base we cannot
follow may declare a signature and reporting "none" would turn a missing answer
into a confident wrong one. The cycle guard is the **path**, as in
`get_property_of_declared_symbol`, for the same reason: no `resolvedBaseTypes`
memo exists to park a sentinel in.

### This cannot desynchronise the printer from the lookup

The obvious trap — the lookup finding an inherited signature the printer never
rendered — **cannot arise**, and the reason is worth recording because it is not
obvious. `render_object_type` is reached only from `get_type_from_type_literal`
and `check_object_literal`; an interface prints by *name* and never renders its
members, and a type literal has no heritage clause. No type that prints
structurally can have an inherited index signature. **If a structural printer for
interfaces is ever added, this paragraph is the one to re-read.**

### The merge gap did not fall out for free, and the absence is stated

Inheritance layers by key type and shadows on collision, so it can never hand
`findApplicableIndexInfo` two signatures with the *same* key. The two applicable
signatures that need merging come from one type declaring both `[k: string]` and
`[k: number]`, which was reachable before this walk existed. Untouched, still
gapped — recorded so the absence is not mistaken for an oversight.

### Two decorations, both caught by running the mutation and not by reading it

The rule in [conventions](../conventions.md#run-the-mutation-do-not-read-it)
earned its place twice more here, on the author's own work:

1. **The shadowing assertion was a decoration.** Dropping the shadow-by-key
   filter left the `string`-key test **green**: two string signatures are
   shadowed incidentally by push order, because `get_applicable_index_info`
   takes the first `find` and own signatures are pushed before inherited ones.
   Only the **`number`**-key case discriminates — there both duplicates reach
   `applicable`, which gaps on two. The test now asserts the number case, and
   the comment says why.
2. **The gap-versus-empty assertion still is one, and is labelled rather than
   counted.** Replacing the `?` on `base_symbols_of` with `unwrap_or_default()`
   leaves it green, because the sole caller maps both `None` and `[]` to no
   answer and both print `error`. The `Option` is justified by the reasoning
   above, not by a test, and saying so is the point — it becomes a real check
   the moment a caller acts on emptiness rather than absence.

The cycle guard, by contrast, is genuinely covered: forcing it false overflows
the stack and aborts with SIGABRT. A bite rather than a clean failure, and the
only signal available, since there is no smaller observation than "the checker
returns".

## The wrong-answer differential, and why its design is written down here

Built 2026-08-05 as a standalone crate **outside** the workspace, because
`tsr-checker` cannot depend on `tsr-conformance` (cycle) and the question needed
the real producer rather than a replica. It found three defects in
`getTypeOfFuncClassEnumModule` — the expando-property answer, the missing
accessor dispatch arm, and the `this`-in-a-JS-constructor population — and its
parent-kind column, folded into the attribution probe, cracked two more.

**The crate itself was ephemeral and is gone.** It lived in a session scratchpad
and nobody had the budget to merge it properly. That is the reason this section
exists: the code was disposable, the design is not, and re-deriving it cost more
than writing it down. Four points, which are the specification for rebuilding it:

1. **Reconstruct each source from the baseline's own interleaved lines** — drop
   the `//// [...]` header, the `=== file ===` marker, and every `>` line; skip
   any baseline with more than one `=== ` marker. No test-case path resolution
   and no `@filename` splitting. Extra blank lines are harmless because the
   comparison is positional, not by line number.
2. **Align with the prefix test.** Upstream's line starts with `{our text} : `
   and the remainder is upstream's type — the same trick `types_shapes` uses, and
   the reason neither side has to split a line that may contain `" : "`.
3. **Isolate *wrong* answers only** — ours ≠ upstream **and** ours ≠ `error`. No
   other instrument separates that population, and it is the one where a defect
   is indistinguishable from a result.
4. **Bucket by more than one fact.** The closure `assertions_for_file` takes
   receives the `NodeId`, so a parallel vector gives `ids[i]` for
   `assertions[i]` and therefore the node kind, the parent kind, and the
   symbol's flags.

### Two columns are worth more than twice one column

Point 4 is the one that earned its place, and by an argument neither column could
make alone. Bucketing by **parent kind** put 4,577 lines under `QualifiedName`,
97.3% of them with a `TypeReference` grandparent. Bucketing by **symbol flags**
put 4,460 under `VALUE_MODULE`. Same population, reached from two different
facts — and agreement between bucketings built on *different* evidence is much
stronger than a large count in either.

The accessor defect is the converse case: **invisible by parent kind** — its
parents were ordinary `GetAccessor` nodes, indistinguishable from the cases that
gap correctly — and obvious by flags, where `METHOD | GET_ACCESSOR | SET_ACCESSOR`
has no business reaching a signature printer. Parent kind names the *construct*;
flags name the *dispatch*. That defect was a dispatch bug, and no amount of
looking at constructs would have found it.

### Two rules for reading any instrument, learned the hard way here

- **A bucket you cannot reproduce is not a false positive.** The accessor bucket
  survived three hand-built fixtures that all came back clean, because all three
  were the wrong shape: a lone accessor already gaps, and only a *merge* reaches
  the arm. It was recorded as "unreproduced" rather than dismissed, which is the
  only reason it was picked up again and fixed. The same shape appeared
  independently in the attribution probe the same day, where an arm reading zero
  was reading zero because it asked the wrong question. **A null from an
  instrument is a fact about the instrument until shown otherwise.**
- **Aggregates rank; per-file dumps diagnose.** Every defect here was identified
  by narrowing to a *single* baseline and printing our assertions beside
  upstream's with node kinds attached. The bucket says where to look; it never
  says what is wrong.
