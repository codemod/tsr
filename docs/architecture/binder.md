# The binder

The binder walks a parsed file once and produces two things:

1. **Symbols and scopes.** A `Symbol` per declaration, filed into the `locals`,
   `members`, or `exports` table its scope demands. This is where `var` hoisting,
   block scoping, and declaration merging happen.
2. **The control-flow graph.** The structure narrowing walks: what was known
   about a reference at the point it was written.

Ported from `internal/binder/binder.go` (2,773 lines) at the pinned commit. The
flow half is roughly two thirds of that file.

Crate: `crates/tsr-binder`. Entry point: `tsr_binder::bind(file, nodes) ->
BindResult`.

---

## Why the binder is a struct with fields

The symbol half used to thread a `Scope` value through the recursion, and that
read well: a scope is a property of a *position* in the tree, so passing it down
and letting it expire on the way out is exactly right.

Control flow is not like that. `current_flow` is a cursor that moves **forward
across siblings** — after `if (a) return;`, the next statement's flow is what the
`if` left behind, not what the enclosing block started with. A by-value parameter
would restore the wrong thing at every statement boundary.

There are nine such cursors:

| field | scope it is saved and restored across |
|---|---|
| `current_flow` | a control-flow container, unless it is an IIFE |
| `current_break_target` | a loop, a `switch`, a labelled statement |
| `current_continue_target` | a loop |
| `current_return_target` | a control-flow container, a `try` with a `finally` |
| `current_true_target` / `current_false_target` | one conditional operand |
| `current_exception_target` | a `try` block, then again a `catch` block |
| `pre_switch_case_flow` | a `switch` |
| `active_labels` | a labelled statement; hidden wholesale by a function |

Each has its own save-and-restore *scope*, and none of them coincides with the
recursion. That is why upstream keeps them as fields with explicit saves, and why
doing anything else would be a rewrite of the algorithm rather than a port of it.
The lexical scope (`container`, `block`, `owner`) moved into fields alongside
them, so `bind_container` is one place that saves both halves.

---

## The flow graph is not the control-flow graph

This is the thing most worth understanding, and the thing a from-scratch
implementation gets wrong.

A faithful CFG of `if (foo()) { x }` has a branch. TypeScript's flow graph does
not — because nothing about `foo()` returning true tells the checker anything
about any type, so a node recording the branch would cost memory and analysis
time and buy nothing. `if (x) { x }` *does* get a condition node, because `x` is
a reference narrowing can act on.

Every predicate in `narrowing.rs` is a filter with that one purpose:

- `is_narrowing_expression` — could narrowing on this condition change a type?
- `is_narrowable_reference` — is this a storage location the checker can track?
- `is_dotted_name` — could this call be an assertion signature?
- `is_potentially_executable_node` — would reaching this actually *run*
  something? (A type alias after a `return` is unreachable and harmless; an
  assignment after a `return` is unreachable and a mistake.)

They are ported arm for arm rather than reconstructed, because the failure mode
is silent. Too permissive and the graph is quietly larger and slower; too strict
and narrowing is quietly lost. Neither crashes, and neither shows up in a test
that only checks the positive cases — which is why the test suite in
`tests/flow.rs` asserts the *absence* of nodes as well as their presence.

The filter is also what makes the graph affordable: 81,713 flow nodes for
419,464 AST nodes across the benchmark fixtures, one per five.

---

## Shapes

Statement by statement, the graph the binder builds. `→` is an antecedent edge;
labels collapse when they have fewer than two antecedents (`finish_label`), so
most of these junctions do not survive into the finished graph.

**`if (c) a; else b;`** — condition splits, both arms merge.

```
      ┌─ TrueCondition(c) → a ─┐
prev ─┤                        ├→ postIf
      └─ FalseCondition(c) → b ┘
```

**`while (c) body`** — a *loop* label, so the back edge is visible. This is what
makes an assignment inside the body affect the next iteration's narrowing.

```
prev ─→ LoopLabel ─→ TrueCondition(c) ─→ body ─┐
          ↑                                     │
          └─────────────────────────────────────┘
                    └→ FalseCondition(c) → postWhile
```

**`a && b`** — `b` is evaluated only when `a` was truthy; `a || b` is the same
picture with the branches swapped. Short-circuit narrowing is entirely this
asymmetry.

**`switch (x) { case … }`** — each clause gets a `SwitchClause` node carrying a
*range* of clause indices, so that `case 1: case 2: body` narrows on both. A
`switch` with no `default` also gets an **empty** range on the exit path, which
is how "the subject matched nothing" is stated; without it a `switch` would look
exhaustive when it is not.

**`try`/`catch`/`finally`** — the one construct whose graph is not a tree, and
the only place a `ReduceLabel` appears.

Any code in a `try` block may throw, but only a *mutation* can change what the
`catch` block sees, so `create_flow_mutation` — not every statement — threads
onto the exception label. Control reaches a `finally` five ways: normal
completion of the try, normal completion of the catch, a `return` from either, or
an exception from either. But:

- analysis that starts *past* the finally must see only the first two (if we are
  past it, it completed normally);
- analysis of an IIFE's returns must see only the third.

So the pre-finally label is given a *reduced* antecedent set at each of those
exits, rather than one set that is right for none of them. That is what a
`ReduceLabel` node carries: a target label and the list to substitute for its
antecedents while flow analysis passes through.

**Parameter and binding-element initializers** are bound out of source order —
initializer before name — and merged: `function f(a = g())` may or may not call
`g`, so the flow after the parameter list is the junction of "the default ran"
and "it did not" (upstream's `bindInitializer`, TypeScript#49759).

---

## Where results go

Upstream writes its conclusions back into the AST: `node.FlowNode`,
`node.Flags |= Unreachable`, `bodyData.EndFlowNode`. None of that is available
here — the AST is immutable and `Sync`
([ADR-0012](../adr/0012-ast-is-sync.md)) and the binder holds only `&NodeTable`.
Each lands in a side table on `BindResult` instead, dense or sparse according to
how many nodes actually have one:

| upstream | here | storage | why |
|---|---|---|---|
| `node.FlowNode` | `BindResult::flow_of` | `Vec<Option<FlowId>>` | every statement and every identifier has one; not sparse |
| `node.Flags & Unreachable`, `HasImplicitReturn`, `ContainsThis`, … | `BindResult::facts` | `FxHashMap<NodeId, NodeFacts>` | a file with no unreachable code and no `this` should pay nothing |
| `bodyData.EndFlowNode` | `BindResult::end_flow` | map | one entry per function |
| `ReturnFlowNode` | `BindResult::return_flow` | map | constructors and static blocks only |
| `FallthroughFlowNode` | `BindResult::fallthrough_flow` | map | `case` clauses only |

`NodeFacts::UNREACHABLE` is *recorded, not reported*. Whether an unreachable
statement or an unused label is an error depends on compiler options the binder
does not have.

---

## Reading a node's parent

Several ported predicates are written upstream as `node.Parent` walks:
`isTopLevelLogicalExpression` climbs through parentheses and `!`;
`IsAssignmentTarget` climbs through array literals and property assignments;
`GetImmediatelyInvokedFunctionExpression` climbs through parentheses to a call.

The tree has no back-edges ([ADR-0003](../adr/0003-tree-plus-side-tables.md)) and
`NodeTable::parent` answers with a `NodeId`, not a `Node`. There is no
id-to-node table, and building one would cost 16 bytes per AST node — 6.7 MiB on
the fixtures — to serve a handful of predicates.

Instead the binder keeps an **ancestor chain**: `Vec<(NodeId, Node)>`, pushed on
the way in and popped on the way out, bounded by source nesting depth. Because
the chain *is* the ancestor path, walking up is an index decrement rather than a
search, and every predicate that climbs does so in O(1) per step.

The invariant this relies on: a predicate is only ever asked about the node being
bound or one of its **direct children**. That holds for every call site — the
predicates are asked about the current node or about a field of it — and
`parent_of` carries a `debug_assert` that checks it against the side table.

---

## Traversal order

`bind_children` dispatches on kind, and for most kinds falls through to
`bind_each_child`, a generic walk via `tsr_ast::push_children`. Three orderings
are deliberate and not source order:

- **Functions first** in a block, a module block, or the source file
  (`bindEachStatementFunctionsFirst`). This is hoisting made visible: a call
  earlier in the block than the function it names has to find a symbol, and a
  single-pass binder gets that only by declaring the functions first.
- **Initializer before name** in a binding element or parameter, per
  ECMAScript's evaluation order.
- **Arguments before callee** for an IIFE, because the arguments are evaluated
  before the function body runs and the body should pick up the flow they left.

`bind_each_child` uses **one shared stack** on the binder rather than a `Vec` per
level: each frame appends above the previous frame's high-water mark and
truncates back to it on the way out. Replacing the per-level allocation is why
parse+bind on `checker.ts` got *faster* (27.99 ms → 25.66 ms) while gaining the
entire flow graph.

---

## What is not built

Named here rather than left to be discovered. Each has a `bd` issue.

- **Destructuring patterns declare no symbols** (`tsr-y4u.3`). `const { a, b } =
  x` should declare two; `declaration_name` returns `None` for a binding
  pattern. The *flow* side does handle patterns — one assignment node per name —
  so narrowing is ready for the symbols when they arrive.
- **Computed property names** (`tsr-y4u.3`). `{ [k]: 1 }` declares a late-bound
  name.
- **Module vs script, and `export`** (`tsr-y4u.3`). Every file binds as a
  script, so top-level declarations are locals rather than exports of a module
  symbol. Needs module resolution.
- **Optional chains** (`tsr-y4u.7`). The flow shapes are ported in full, but the
  parser records the `?.` token without setting `NodeFlags::OPTIONAL_CHAIN`, so
  `is_optional_chain` is always false and `a?.b` currently gets the graph of
  `a.b`. Every optional-chain path in `binder.rs` is unreachable until that flag
  is set, at which point it becomes live with no further work.
- **Strict-mode and contextual-identifier diagnostics** (`tsr-y4u.8`). Upstream
  reports `with` in strict mode, `eval`/`arguments` misuse, octal literals, and
  private-identifier placement. The binder here emits only
  duplicate-identifier.
- **Deep recursion** (`tsr-el3`). `bind` recurses to tree depth. Measured
  2026-08-04: 10,000 nested blocks bind fine; 50,000 overflow — but in the
  *parser*, before the binder is reached. The exposure is real and pre-existing;
  the parser fails first.

---

## Testing

Three layers, each answering a different question.

**`tests/bind.rs`** — scoping questions, asserted through the resolved tables.
"`x` in this block resolves to the `let`, not the outer `var`", not "there are
four symbols".

**`tests/flow.rs`** — narrowing questions, asserted through the graph. Since
there is no checker yet, the only available vocabulary is *which* flow node is in
effect at a given identifier and what its antecedents are — which is exactly what
a narrowing query asks. Includes negative assertions (`if (f())` produces no
condition node) because the failure mode of a filter is a graph that is quietly
too big and still passes every positive test.

**`crates/tsr-conformance`, suite `binder_symbols`** — judged against upstream's
own `.symbols` baselines over the 12,444-case corpus. Currently 7,751/8,455
(**91.67%**). See
[ADR-0006](../adr/0006-conformance-oracle.md) for why the baselines are the right
oracle and [conformance.md](conformance.md) for what the suite does and does not
compare.

The flow graph has **no conformance oracle at all**: upstream publishes no
baseline of it, and the checker that would exercise it does not exist. That is a
real gap, and it is why `tests/flow.rs` is written as carefully as it is.

### How the number moved, and what it cost

Every step was chosen by capturing all failing cases and bucketing each
individual complaint, not by guessing. That mattered: the first bucketing found
that **40% of failures were the oracle rather than the binder**, and two of the
five biggest wins turned out to be in the harness.

| | rate | what changed |
|---|---:|---|
| start | 62.05% | |
| forward-scanning full starts, name spelling normalised | 69.61% | harness — see [conformance.md](conformance.md) |
| destructuring patterns declare their names; catch variables block-scoped | 72.94% | binder |
| parameter properties; namespace exports | 73.95% | binder |
| `export` modifier kept by the parser | 71.04% | **regression** |
| exported members get a local *and* an export | 75.42% | binder |
| anonymous containers own their members | 83.15% | binder + harness |
| symbols indexed by every dotted suffix | 87.60% | harness |
| `namespace A.B {}` desugared into nested modules | 88.39% | parser |
| computed names that are literals declare statically | 88.78% | binder |
| JSX attributes declare properties | 90.51% | binder |
| a class expression displayed under the variable it is assigned to | 90.72% | harness |
| index signatures (`__index`) and named function expressions | 92.04% | binder |
| single-quoted names unquoted | 92.18% | harness |
| **multi-file cases compared unit by unit** | 91.67% | harness — *denominator +834* |

The regression is the instructive one. Preserving the `export` modifier let
namespace members route into the namespace's `exports` — correct, and it broke
223 cases, because upstream's `declareModuleMember` deliberately creates **two**
symbols for an exported member: a local and an export. Its comment explains why
at length, and the reason it is not an optimisation is exactly what the failures
showed: an unqualified reference inside the namespace resolves to the local, so
creating only the export loses every one of them.

### The denominator changed, on purpose

At 92.18% the rate was measured on a corpus that silently excluded every
multi-file test — 1,153 cases, 13% of everything with a `.symbols` baseline,
skipped as "needs per-file symbol attribution". Each baseline section names a
unit, so each unit is now bound and compared on its own. That is not an
approximation: there is no program and no cross-file linking yet, so a symbol
declared in `a.ts` is genuinely not visible from `b.ts`, and the comparison
already dropped any symbol whose declarations live in another file.

| | before | after |
|---|---:|---:|
| denominator | 7,621 | **8,455** |
| passing | 7,025 | **7,751** |
| rate | 92.18% | 91.67% |

The rate fell half a point and that is the honest direction: it now describes 11%
more of the corpus, and 87% of the newly-tested cases pass. Quoting the old
number against the new one would be comparing two different corpora.

### What the remaining 704 failures are, and why this is near the ceiling

Classified case by case against the actual sources, not by name shape:

| Cause | Cases | Status |
|---|---:|---|
| Late-bound computed names | ~344 | **blocked on the checker** |
| `import X = Y` alias resolution | ~84 | **blocked on a resolver** (`tsr-y4u.12`) |
| Cross-file / module export | ~82 | blocked on module resolution |
| Long tail, many distinct causes | ~89 | each below ~10 cases |
| JavaScript binder features | ~45 | six sub-features, largest 16 cases |
| `export default` symbol merging | ~15 | blocked on module-vs-script |

**Roughly 72% of what remains is hard-blocked**, and no tractable cause above
~16 cases is left. That is the useful conclusion: further movement on this metric
comes from the checker, the resolver, and module resolution — not from more
binder work. The JavaScript features (`this.x = …`, `module.exports`, expando
assignments, prototype assignments) are the largest genuinely-available block and
are filed, but they are six separate features for 0.5 points.

### What was already right

Two things were checked rather than assumed, after an earlier version of this
document mislabelled the wrong-lines bucket as "declaration merging across module
blocks":

- **Declaration merging works.** `namespace Outer {} namespace Outer {}` produces
  one symbol with two declarations and merged exports; two `interface I`
  declarations produce one symbol with merged members. It never appeared in the
  failures because it was never broken.
- The bucket was in fact 65 alias-resolution cases, 35 JSX-attribute cases, and a
  long tail. The JSX half was a real and self-contained gap — JSX attributes
  declared no symbols at all — and closing it moved JSX from 55 failing cases to
  5.

---

## Performance

Measured on the four benchmark fixtures (5.56 MB of source, 419,464 AST nodes)
with `cargo bench -p tsr-binder --bench bind` and
`cargo run -p tsr-binder --example rss --release`. See
[ADR-0009](../adr/0009-performance-gate.md) for the method and
[performance.md](performance.md) for the typescript-go comparison.

| | before flow graph | with flow graph |
|---|---|---|
| parse+bind, `checker.ts` | 27.99 ms | **25.66 ms** |
| parse+bind, `dom.generated.d.ts` | 10.04 ms | **8.71 ms** |
| binder peak RSS, four files | 12,288 KiB | **15,616 KiB** |

The time improvement is the shared child stack, not the flow graph; the memory
increase (3,328 KiB, 27%) is the flow graph, and
[ADR-0014](../adr/0014-flow-graph-representation.md) accounts for it byte by
byte.

**The binder's RSS has no typescript-go number beside it.** `benches/go/rss_test.go`
covers parsing only, so the figures above are absolute rather than comparative.
Filed as `tsr-y4u.9`.
