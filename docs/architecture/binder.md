# The binder

The binder walks a parsed file once and produces two things:

1. **Symbols and scopes.** A `Symbol` per declaration, filed into the `locals`,
   `members`, or `exports` table its scope demands. This is where `var` hoisting,
   block scoping, and declaration merging happen.
2. **The control-flow graph.** The structure narrowing walks: what was known
   about a reference at the point it was written.

Ported from `internal/binder/binder.go` (2,773 lines) at the pinned commit. The
flow half is roughly two thirds of that file.

Crate: `crates/tsr-binder`. Entry point: `tsr_binder::bind(file, nodes, file_name)
-> BindResult`.

---

## Module or script

A file binds one of two ways, and the difference is not cosmetic.

A **script**'s top-level declarations are globals: they go in the file's `locals`
and there is nothing to export them from. An external **module** has a symbol of
its own — named after its path with the extension removed — and an exported
declaration is filed **twice**: a local carrying only `EXPORT_VALUE`, and an
export on the module symbol carrying the real flags. That duplication is
upstream's (`declareModuleMember`, `internal/binder/binder.go:373`) and it is not
an optimisation to be undone; see the regression recorded below.

A file is a module when a top-level statement is an `import`, an `export`, an
`export =`, or carries the `export` modifier — upstream's
`isFileProbablyExternalModule`, which the *parser* runs there and the binder runs
here. Why here, and why the file name is a parameter rather than a field on the
node, is [ADR-0015](../adr/0015-file-name-is-a-bind-input.md).

**`export default` is what makes this visible.** The *export* half of a default
export is always named `default`, whatever the declaration was called
(`declareSymbolEx`, `binder.go:154`). That is the whole mechanism by which

```ts
export default function foo(value: number): number
export default function foo(value: string): string
export default function foo(value: string | number): string | number { return 1 }
export default interface Foo {}
```

is **one symbol with four declarations** — `foo` and `Foo` are different names, so
nothing else would merge them. The local halves keep the written names, which is
what an unqualified `foo` inside the file resolves to. An *unnamed* default
(`export default class {}`) gets no local at all: there is no name to file one
under, and upstream says so in as many words.

Three things came out of the parser rather than the binder, because they were
never recorded:

- `export default class C {}` **dropped the `default` modifier**, keeping only
  `export`. Nothing downstream could then tell it from `export class C {}`.
- `export as namespace N` was parsed as an `ExportAssignment`, which made it
  indistinguishable from `export default N`. It is now a
  `NamespaceExportDeclaration`, and its symbol goes in
  `BindResult::global_exports` — upstream's `SourceFile.GlobalExports` — because
  a UMD global is a name the module claims *as a script*, not something
  `import { N }` may resolve.
- `export = x` and `export default x` produced no symbol at all;
  `ExportAssignment` now declares one under `export=` or `default`.

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

## JavaScript declares things by assigning to them

A `.js` file has no `export`, no class fields, and no type annotations, so the
forms that mean "declare" there are assignments. Upstream reads three of them as
declarations (`GetAssignmentDeclarationKind`), and the binder does too — gated on
the file being JavaScript, because in a `.ts` file the identical source is an
ordinary write to a global:

| written | means | lands in |
|---|---|---|
| `module.exports = x` | `export = x` | the file's exports, under `export=` |
| `exports.x = 1`, `module.exports.x = 1` | a named export | the file's exports |
| `this.x = 1` in a class member | a property of the class | the class's members |
| `f.x = 1` | an **expando** property | the exports of whatever `f` names |
| `Object.defineProperty(f, "x", …)` | the same, written as a call | as above |

The last two are **not** gated on the file being JavaScript: TypeScript reads a
property assigned to a function as a declaration too, which is what makes
`function f() {}; f.cache = new Map()` type-check.

The first two also make the file a **module**: a `.js` file with no `import` or
`export` is a script until a `module.exports` appears, at which point
`set_commonjs_module_indicator` gives it a symbol part-way through the walk. That
is why the file's symbol is a field on the binder and not the `owner` cursor —
by the time a `module.exports =` nested inside a function is reached, `owner` is
something else entirely.

Once a file is CommonJS, `module` and `exports` are declared as locals of it
(`declareCommonJSVariable`), with `exports` also a member of `module`. Nothing in
the source declares them, which is exactly why the binder must: every reference
to either would otherwise resolve to nothing.

### Expando assignments are bound in a second pass

`f.x = 1` names a target the binder has not necessarily reached — `f` may be
declared further down the file — so upstream records each one with the scope it
was written in and binds them all once the walk is over
(`bindDeferredExpandoAssignments`). Three things about that pass are worth
knowing, because each is a place a from-scratch implementation would do more
work than upstream and get a different answer:

- **The lookup is two containers, not a scope chain.** `lookupName` reads one
  container's own locals and its symbol's exports, and `bindDeferredExpandoAssignment`
  tries the block scope and then the function scope. A name three scopes out is
  simply not found.
- **The properties land on the *initializer*, not on the declaration.**
  `const g = function () {}` puts `g.y` on the function expression's symbol,
  because that is the thing with a call signature. `getInitializerSymbol` accepts
  a function declaration, a `const` (or, in JavaScript, any) variable
  initialised with a function, an arrow, a class expression, or an *empty
  unannotated* object literal — and nothing else.
- **A real declaration wins.** "We declare expandos only when there are no
  non-expando declarations for that name": a declared `static x` is not merged
  into or displaced by `C.x = 1`.

Reaching the initializer needed one thing upstream gets free. It reads
`declaration.Initializer()`, and the tree here has no parent-to-child edge to
follow from a `NodeId` ([ADR-0003](../adr/0003-tree-plus-side-tables.md)). Rather
than build an id-to-node table for it, the binder records the initializer's id on
the way past — in the one place that has both the node and its id — and only for
initializers that are expando-shaped, so the map holds a handful of entries.

**`C.prototype.x = 1` still declares nothing**, here and upstream: the lookup
reaches the class's `prototype` export, which has no value declaration, so
`getInitializerSymbol` gives up. Upstream's constructor-function handling, which
is what would make it work, is marked `!!!` — unimplemented — in the Go port too.
Adding the `prototype` symbol to classes (TypeScript 1.0 spec §8.4) was tried and
measured at **zero** cases, so it is not there; it belongs with the
duplicate-identifier work, where our members-versus-exports split would have to
change for it to do the job it does upstream.

**`this.x` is replaceable by a method.** A property declared this way loses
outright to a real declaration of the same name rather than merging with it,
because `this.m = this.m.bind(this)` in a constructor must not turn the method
into two declarations. That is upstream's `isReplaceableByMethod`, and it is the
one merge rule in the binder that discards a declaration.

### The parser was losing every contextual keyword used as a name

`module.exports` did not work at first, and the reason was not in the binder:
`module` was being parsed as a `KeywordExpression`, which carries a *kind* and no
text. So did `type`, `of`, `as`, `declare`, `async`, `get` — every non-reserved
keyword in expression position. `const x = type;` referred to nothing.

A keyword is reserved or it is contextual, and only a reserved one (`this`,
`super`, `true`, `false`, `null`) is the keyword when it appears as a value.
Upstream falls through to `parseIdentifier()` for the rest; so does the parser
now. It was worth 14 conformance cases on its own, well beyond the CommonJS work
that found it, and it is the reason the flow graph grew from 81,713 nodes to
83,690: an identifier is a narrowable reference and a keyword expression is not,
so those references now get flow nodes they should always have had.

---

## Late-bound names still get a symbol

`class C { [Symbol.iterator]() {} }` declares a member whose *name* nobody can
know until the checker evaluates `Symbol.iterator`. The binder used to declare
nothing for it, on the reasoning that a symbol under a guessed name would be
unreachable by any reference. That reasoning was right about the name and wrong
about the symbol: upstream creates one, and creating none loses more than it
saves.

Upstream's `bindPropertyOrMethodOrAccessor` routes a dynamically-named member to
`bindAnonymousDeclaration` with the name `__computed`
(`ast.InternalSymbolNameComputed`), and three properties of that symbol are the
whole design:

- **It is in no symbol table.** Two `[k]`s in one class are two symbols, not one
  merged under `__computed`. Which of them collides — if either — is a question
  about the *values* of the expressions, so it is the checker's answer.
- **It is parented to the container** when it is a class member or enum member,
  which is what lets the checker attach a resolved name to the right class, and
  what makes the baselines print `C[Symbol.iterator]` rather than a bare name.
- **The declaration keeps its expression**, which is what late binding reads.

The last one needed a side table here. The checker will be handed a `NodeId` and
the computed name is a *child* of the declaration, an edge the tree does not have
([ADR-0003](../adr/0003-tree-plus-side-tables.md)), so `BindResult::computed_name`
records it — for every computed name, static (`['a']`) or not, because upstream
prints them all back the way they were written.

**This is the declaration half only.** What is still missing is late *binding*:
`class C { a: string; [k]: number }` where `k` is `"a"` should end up one symbol
with two declarations, and here it is two. That needs the checker, and the four
conformance cases that still fail on computed names are all of exactly that shape.

`[-1]` remains late-bound. Upstream treats a signed numeric literal as static and
builds the name by concatenating the operator with the operand, which needs an
owned string where every name here borrows from the source.

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

The filter is also what makes the graph affordable: 83,690 flow nodes for
419,571 AST nodes across the benchmark fixtures, one per five. (Measured
2026-08-04. The earlier figure of 81,713 was correct for the tree as it was then
parsed: contextual keywords in expression position were `KeywordExpression`s,
which are not narrowable references and so got no flow node. Fixing that in the
parser added 1,977 — nodes the graph should always have had.)

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

- **Late-bound computed property names** (`tsr-y4u.3`). `{ ['a']: 1 }` declares
  `a` statically, but `[k]` and `[Symbol.iterator]` name whatever the expression
  evaluates to. Upstream files them under an internal `__computed` name and
  resolves them in the checker; the binder declares nothing, because a symbol
  under a guessed name would be unreachable by any reference. This is the single
  largest remaining cause of `binder_symbols` failures and it is **blocked on the
  checker**.
- **`import.meta` as a module indicator** (`tsr-y4u.3`). A file whose only
  module-ness is a mention of `import.meta` binds as a script. Detecting it needs
  a full-tree walk under module settings the binder does not have; see
  [ADR-0015](../adr/0015-file-name-is-a-bind-input.md).
- **`export * from "m"`**. Upstream collects every star export in an `__export`
  symbol; nothing is declared for one here. `export * as ns from "m"` *is*
  declared.
- **Constructor functions** (`tsr-y4u.16`). `function C() { this.x = 1 }` should
  declare `x` on `C`, and `C.prototype.m = …` should declare `m` on its
  prototype. Upstream marks both `!!!` — unimplemented — in the Go port, so
  neither is ported here either.
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
own `.symbols` baselines over the 12,444-case corpus. Currently 8,212/8,455
(**97.13%**). See
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
| module vs script: module symbol, `export` routing, `default` merging | 92.13% | binder + parser + harness |
| JavaScript: `module.exports`, `exports.x`, `this.x`, CommonJS locals | 92.48% | binder |
| contextual keywords in expression position parse as identifiers | 92.64% | **parser** |
| expando assignments and `Object.defineProperty`, bound in a deferred pass | 92.88% | binder + harness |
| late-bound names get a `__computed` symbol | 97.13% | binder + harness |

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

### What module-vs-script actually cost and bought

**39 cases fixed, 0 broken**, measured by capturing every failure before and
after and diffing the two lists by case name. The 39 are: 10 `export default`
merging cases, 10 `export as namespace` cases that had no symbol at all, and 19
elsewhere in the corpus that the export routing reached.

**The prediction was 97, and it was wrong by a factor of two and a half.** The
prior classification below attributed ~82 failures to "cross-file / module
export" on the strength of the *names* in the failure text. Re-bucketing them
against the sources shows most of that group is something else: 73 are
`import X = Y` alias resolution, and a further block is the JavaScript binder
(`module.exports`, expando assignments). Only about 20 were module-vs-script, and
the rest of the 39 came from cases nobody had classified into that bucket. The
lesson is the same one this section keeps recording, applied one level deeper:
bucketing by *reason string* is still bucketing by name shape.

### What the JavaScript forms cost and bought

**43 cases fixed, 0 broken**, in two measured steps. The three assignment forms
and the CommonJS locals were worth 29; the contextual-keyword parser fix they
uncovered was worth a further 14, none of them JavaScript cases.

The estimate in `tsr-y4u.13` was "45 failing cases across six sub-features,
largest 16". Three sub-features covered 43 cases — more than the arithmetic
allowed, because the parser bug behind `module.exports` was also silently
costing cases nobody had attributed to JavaScript at all. That is the third time
this session a cause named from failure text turned out to be somewhere else.

### "Blocked on the checker" was wrong about 359 cases

The table below used to read *"late-bound computed names — ~371 — blocked on the
checker"*, and that was the largest single line in it for three revisions of this
document. It was wrong, and the way it was wrong is worth keeping.

The reasoning was: the *name* of `[Symbol.iterator]` cannot be known without the
checker, therefore nothing can be done. Both halves are true and the conclusion
does not follow. Upstream does not know the name either at bind time; it creates
a symbol called `__computed` anyway and resolves the name later. Once the binder
does the same, the baselines match — because `symbolToString` prints a computed
member as the *source text of its name*, `C[Symbol.iterator]`, whether or not the
name was ever resolved.

**359 cases fixed, 0 broken.** The split between the two halves was measured
rather than assumed: with the harness change in place and the binder's
`__computed` symbol switched off, the rate is 92.88% — exactly where it was. The
whole gain is the binder creating symbols; the harness change only makes them
visible.

What the suite no longer tests is stated in
[conformance.md](conformance.md#what-is-approximate-and-what-is-not-covered).
The residual four cases are the part that is genuinely blocked, and they all have
the same shape: a late-bound name that should have *merged* with a declared
member.

### What the remaining 243 failures are

Classified against the actual sources:

| Cause | Cases | Status |
|---|---:|---|
| `import X = Y` alias resolution | ~73 | **blocked on a resolver** (`tsr-y4u.12`) |
| Module-shaped, various | ~34 | mostly cross-file, blocked on module resolution |
| Late-bound *merging* | 4 | **blocked on the checker** (`tsr-y4u.11`) |
| JavaScript | 5 | JSDoc `@overload`/`@typedef` |
| Long tail, many distinct causes | ~127 | each below ~10 cases |

The long tail is now the largest bucket, which is the useful signal: there is no
remaining single cause worth a session on its own, and the next real movement
comes from the resolver and from module resolution.

Two module-shaped things are known to be available and small. `export default x`
where `x` is an identifier should display under `x`'s name, because upstream's
`getNameOfDeclaration` returns the expression for an `ExportAssignment`; that is
3 cases and needs the harness to reach a node it currently cannot (it has a
`NodeTable`, not an id-to-node map). `export * from "m"` declares nothing.

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
