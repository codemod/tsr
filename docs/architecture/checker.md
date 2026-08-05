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

1. **`bd tsr-4sc.13` — the remaining expression forms**, in measured order:
   binary, property access, call, element access. 40% of gaps and the largest
   single item in the corpus. Property access and call need 2 for anything but
   the simplest receivers, so the honest first slice is the binary operators —
   **done, and it moved the gradient 22.39% → 28.72%**; see below. Property
   access (23,732 lines), call (15,867) and element access (13,549) remain, and
   all three now wait on 2.
2. **`bd tsr-4sc.7` — `getTypeFromTypeNode` and `getDeclaredTypeOfSymbol`.** 21%
   of gaps sat on an annotation. **First slices done** — named types and
   anonymous object types, 28.72% → 34.06%; see below. Members, heritage and
   instantiation remain, and they are what still gates 3, 5 and 7.
3. **`bd tsr-4sc.8` — `getTypeOfFuncClassEnumModule`.** 12.5% of gaps are a
   symbol whose kind has no type at all: 7,485 function declarations, 7,227
   classes, 5,569 modules.
4. **`bd tsr-tl8` — the member-name defect.** Worth no lines today, and ranked
   above things that are, because it is the one place this port can answer
   *wrongly* where a gap belongs.
5. **`bd tsr-4sc.1` — printing gaps.** Literals are 74% right and 620 of their
   misses are wrong answers rather than gaps — `true` → `boolean` is 181 of
   them, which is a widening defect and not a printing one. Small, and now
   measurable line by line.
6. **`bd tsr-9or.1` — lib files.** Demoted from third by the measurement above.
7. **`bd tsr-4sc.9` — unions**, which is also what lets `boolean` stop being a
   fake intrinsic, and **`bd tsr-4sc.10` — `typeof` queries**, nearly free once
   2 and 3 land.
8. **`bd tsr-4sc.11` — control-flow narrowing.** Still last, and still probably
   underranked: it changes the *correctness* of reference lines everywhere and
   the histogram cannot see it, because a line we answer `error` on would be
   wrong with or without narrowing. It becomes measurable once 1 and 2 land, and
   should be re-ranked then rather than now.
9. **`bd tsr-bb4.1` — per-configuration runs.** Returns 1,397 cases to
   `checker_types`. Worth doing when the rate is high enough that the denominator
   matters; doing it now only makes the number look worse for no information.

Not in the ranking on purpose: **`bd tsr-el3.2`** (upstream's algorithmic
recursion limits) is a standing constraint rather than a task — port each limit
*with* the code it belongs to. And **`bd tsr-pum.11`** (~2,675 parser
over-reports) is 9 points of the `diagnostics` ceiling but ~300 separate
investigations with no dominant cause, which scores poorly until the checker
emits diagnostics at all.

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
| anonymous object types | **571** | **34.06%** |

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

### What is left under `bd tsr-4sc.7`

From the histogram, after this work:

```text
13,710  generic references — instantiation, with upstream's depth 100 and
        count 5,000,000 guards ported alongside it (bd tsr-el3.2)
12,065  array types — upstream models `T[]` as a reference to the global
        `Array` interface, so this one genuinely does want lib files
        (bd tsr-9or.1) rather than another type that prints alike
24,383  object-literal answers still missing, nearly all from expressions
        rather than annotations
         qualified names (`M.I`), which need resolveEntityName through
        module exports
```

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
