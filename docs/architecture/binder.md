# The binder

The binder walks a parsed file once and produces two things:

1. **Symbols and scopes.** A `Symbol` per declaration, filed into the `locals`,
   `members`, or `exports` table its scope demands. This is where `var` hoisting,
   block scoping, and declaration merging happen.
2. **The control-flow graph.** The structure narrowing walks: what was known
   about a reference at the point it was written.

Ported from `internal/binder/binder.go` (2,773 lines) at the pinned commit. The
flow half is roughly two thirds of that file.

Crate: `crates/tsr-binder`. Entry point: `tsr_binder::bind(file, nodes, info) ->
BindResult`, where `info` carries the file's name and text —
[ADR-0016](../adr/0016-file-info-not-a-file-name.md).

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

### Which symbol table a declaration goes into

`classify` returns a `Destination` keyed on the declaration's **node kind**.
Upstream keys the table on the **container's** kind instead
(`declareSymbolAndAddToSymbolTable`,
`vendor/typescript-go/internal/binder/binder.go:428-447`) and never looks at the
declaration. The two agree for every declaration kind that only ever occurs in one
kind of container, which is nearly all of them.

Type parameters are the exception — the same node appears under a class, an
interface, a function, a type alias, a mapped type, and every signature, and
upstream files it in a different table in each — so `classify` special-cases them
against the parent. That divergence, why it was not fixed by converting to
upstream's model, and the `SymbolFlags::excludes()` corrections found with it are
[ADR-0023](../adr/0023-the-symbol-table-comes-from-the-container.md).

Two more per-kind answers were wrong the same way, and are
[ADR-0024](../adr/0024-static-members-and-block-scoped-declarations.md):

- A **static** class member is an *export* of the class, an instance member a
  *member* of it (`declareClassMember` splits on `ast.IsStatic`, `binder.go:415`).
  Sharing one table merged `static m()` with `m()` into a single symbol.
- `locals_owner` decides between the enclosing block and the enclosing function.
  Upstream fixes that per kind by which of two functions binds the declaration:
  `bindBlockScopedDeclaration` → `GetLocals(b.blockScopeContainer)`
  (`binder.go:1249`) takes block-scoped variables, classes, interfaces, type
  aliases, enums and **function declarations**; everything else takes
  `GetLocals(b.container)` (`binder.go:444`). Four of the six were missing.

The failure it caused is worth keeping in mind when reading `classify`: a class is
`IS_CONTAINER` *without* `HAS_LOCALS` (matching `GetContainerFlags`), so
`Destination::Locals` for a type parameter resolved to the enclosing **file**, and
the `T` of `class A<T>` merged with the `T` of `class B<T>` into a single symbol.

A class **expression** goes in no table at all, named or not:
`bindClassLikeDeclaration` splits on the kind, not on the name
(`binder.go:942-951`), sending every class expression through
`bindAnonymousDeclaration`. The name of `const C9 = class C { }` is visible only
inside it — the same rule as a named function expression. See
[ADR-0026](../adr/0026-class-expressions-and-multiple-default-exports.md).

A fourth wrong-table defect sat one level further out, in
`is_exported_from_container` rather than in `classify`: an **export specifier** is an
export of its container unconditionally, and there is no `export` modifier on the
specifier to find — the keyword belongs to the `export { … }` declaration above it.
Asking for the modifier made `export { a as a1 } from "m"` a *local*, colliding with
the local of `import { a as a1 } from "m"`. See
[ADR-0025](../adr/0025-an-export-specifier-is-an-export.md).

**Three of those four fixes did not move `binder_symbols` at all.** It sat at
8,278/8,449 through ADR-0023 and ADR-0024 while three separate defects that merged
unrelated declarations into one symbol were removed; the export-specifier fix moved
it to 8,282. So the suite is a *weak* instrument for this class of bug, not a blind
one: its `.symbols` baselines do not distinguish a static member from an instance
one, a block's locals from its function's, or one class's type parameter from
another's. **Do not read a flat `binder_symbols` as evidence that a symbol-table
change is safe, or that the tables are right.** What caught all four was a
diagnostic they happened to produce, bucketed by declaring construct
(`examples/ts2300_constructs.rs`); what pins them now is unit tests in
`crates/tsr-binder/tests/bind.rs`, each verified to fail with the fix reverted.

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
own `.symbols` baselines over the 12,444-case corpus. Currently 8,278/8,449
(**97.98%**). See
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
| line comments counted as leading trivia; bracket spellings not over-reduced; escaped identifiers decoded | 97.67% | harness |
| a second `static` is a member name, not a modifier | 97.82% | **parser** — *denominator −6* |
| anonymous classes and functions display as upstream writes them; unreadable names get a symbol; JSX namespaced names declare | 97.98% | binder + harness |

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

### A sixth of the remaining failures were the harness again

The 243 failures at 97.13% were classified case by case, and three of the buckets
turned out to be the *measurement* rather than the compiler. Each was sized by
making the change and re-running, not by inspection:

| Cause | Cases | What it was |
|---|---:|---|
| Full start ignored `//` comments | 20 | The forward scan is exact, but where a context-free scan diverges from the parser it falls back to a backwards walk, and that walk handled `/* */` only. A comment above a declaration is trivia *of that declaration*, so upstream reports the declaration two lines above where we did. |
| Bracket spellings over-reduced | 18 | `static_bracket_name` accepted anything *starting* with a digit or a quote, so the baseline's `[0 + 1]` became the member name `0 + 1` and `["+" + bar]` became `"+"bar`. It has to be the whole text. Only visible once computed names produced symbols to compare against. |
| Escaped identifiers | 6 | `var \u0061` declares `a`; the baseline prints the spelling. The same reduction the quote handling already did. |

That is the third time this session that a bucket named from failure text turned
out to be somewhere other than where it was filed. The pattern is consistent
enough to state as a rule: **when a bucket is large and uniform, suspect the
oracle before the compiler.**

### The denominator moved by six, and not on purpose

`class C { static static }` declares a static member *called* `static`. The
parser used to decide whether a modifier keyword was really a name with a
blacklist of what could follow it, which cannot reach that case — the second
`static` is followed by `}` in one test and by `[x: string]: string` in another.
Upstream tests the opposite way (`tryParseModifier`): a **whitelist** of what may
follow a modifier, plus `hasSeenStaticModifier` — a second `static` is never one
— plus a same-line requirement for every modifier except `static` itself, and
explicit cases for `export` (which a decorator may follow) and `default`.

Porting that fixed 3 binder cases and 2 decorator cases that had been silently
mis-parsed (`@dec export @dec class C {}` lost the class entirely). It also made
the parser report errors on 6 cases where it had previously mis-parsed in
silence — `static static p: string` is an error upstream too — and the suite
skips any unit it cannot parse, so those 6 left the judged denominator: 8,455 →
8,449.

That is a real cost and it is recorded here rather than absorbed. The suite's
rule ("a file we cannot parse tells us nothing about the binder") is right, but
it means a parser that becomes *more* faithful can shrink the denominator. The
6 cases were failing before, so nothing that passed was lost.

### What alias resolution actually needs, and why it is not a session's work

The previous revision of this section called alias resolution "the only bucket
worth a session", which implied it was actionable. Reading the baselines says
otherwise, and the mechanism is worth writing down because the issue that tracks
it (`tsr-y4u.12`) described it wrongly.

```ts
export namespace m { export class c {} }
import a = m.c;
```

```
>a : Symbol(a, Decl(…, 3, 1))
>c : Symbol(a, Decl(…, 0, 20))     <- the class, printed as `a`
```

The alias symbol `a` is exactly what we produce: one declaration, the import.
What we do not produce is the second line — upstream resolved the reference `c`
to the **class**, and then printed the class under the name **`a`**, because
`symbolToString` names a symbol by the shortest chain accessible *from the
reference site* (`getAccessibleSymbolChain`), and one identifier beats `m.c`.
Since the suite keys by name, the expected lines for `a` are the union of the
alias's and the class's.

So the bucket needs two things, and the second is the expensive one: alias
resolution *and* accessible-name computation. Neither is binder work. It is
~73 cases and it moves when the checker does.

### What the remaining 171 failures are

| Cause | Cases | Status |
|---|---:|---|
| `import X = Y` alias resolution | ~73 | **blocked on the checker** (`tsr-y4u.12`) |
| Module-shaped, various | ~34 | mostly cross-file, blocked on module resolution |
| JSDoc declarations | ~9 | `@typedef`, `@overload`, `@template` — the reparser |
| Late-bound *merging* | 4 | **blocked on the checker** (`tsr-y4u.11`) |
| Numeric name normalisation | ~5 | needs a *synthesised* name; see [ADR-0016](../adr/0016-file-info-not-a-file-name.md) |
| Long tail, many distinct causes | ~46 | each below ~5 cases |

**On the 553 cases skipped for parse errors**, which an earlier revision of this
section called "binder coverage the parser is hiding": measured 2026-08-04, that
was wrong in proportion. 562 unit-level failures are files **upstream also errors
on** — the suite skips them under "a file we cannot parse tells us nothing about
the binder", which is a rule about the harness, not a gap in the parser. Only 31
are files upstream parses cleanly, and those are the same ones
`parser_typescript` already counts. Recovering the first group means judging a
unit that parses *with* errors against the symbols the baseline names, which is
`tsr-y4u.17` and needs its own reasoning about what a skip means.

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

---

## Name resolution: `resolve_name`, and why it needs a meaning

`BindResult::resolve` — since replaced by `resolve_name`, see below — walked a
node's ancestors consulting each one's `locals` and nothing else. That is the
lexical half of upstream's `(*NameResolver).Resolve`
(`internal/binder/nameresolver.go`), and for a long time it was enough, because
every construct the checker had reached files its declarations in a `locals`
table.

Type parameters are the exception, and it is not a small one.

### The forcing constraint

`crates/tsr-conformance/examples/types_shapes.rs` measured 8,229 gap lines from names
in *type* position that do not resolve. The four commonest are `T` (1,841),
`U` (233), `V` (161) and `K` (95) — 2,330 lines between them, and the real cost is
larger, because every member type that mentions `T` is a gap until `T` resolves.

`bd tsr-y4u.21` diagnosed this as a **binder** defect: a class's or interface's
type parameters were said to be in no scope at all. That diagnosis came from a
`lookup_local` sweep, and `lookup_local` reads only `locals`. Probing `members`
too (`crates/tsr-binder/examples/type_parameter_scope.rs`) says the opposite:

```text
declare function f<T>(p: T): void;   T in the function's locals
type A<T> = { p: T };                T in the alias's locals
declare const g: <T>(p: T) => void;  T in the signature's locals
interface I<T> { p: T }              members[T] of `I`,       T.parent = I
class C<T> { p: T; }                 members[T] of `C`,       T.parent = C
declare class D<T> { m(p: T) }       members[T] of `D`,       T.parent = D
const E = class<T> { p: T; };        members[T] of `__class`
```

**The binder was already right, and matches upstream exactly.**
`declareSymbolAndAddToSymbolTable` switches on the *container's* kind and sends a
declaration inside a class or class expression to `declareClassMember` →
`GetMembers(container.Symbol())`, and one inside an interface to
`GetMembers(container.Symbol())` (`internal/binder/binder.go:429`–`441`). A type
parameter is a declaration like any other. ADR-0023 landed this. Upstream even
comments on how odd it is, at `internal/checker/symbolaccessibility.go:766`:
*"Type parameters are bound into `members` lists so they can merge across
declarations. This is troublesome, since in all other respects, they behave like
locals :cries:"*.

That is the ninth time in this project that an expectation and the implementation
disagreed and the implementation was right. The probe cost twenty minutes; the
fix it would have prompted would have broken a table that was correct.

### What was actually missing

Upstream's loop has an arm this port did not have:

```go
case ast.KindClassDeclaration, ast.KindClassExpression, ast.KindInterfaceDeclaration:
    result = r.lookup(r.getSymbolOfDeclaration(location).Members, name, meaning&ast.SymbolFlagsType)
```

with two rules attached to it. `BindResult::resolve_name` is that arm, plus both
rules:

- **`isTypeParameterSymbolDeclaredInContainer`** (`nameresolver.go:477`). A class
  and an interface of the same name merge into one symbol and share one members
  table, so the `T` of `interface C<T>` is reachable from `class C`'s node.
  Upstream ignores a type parameter whose declaration is parented elsewhere and
  keeps walking outward. Without it, `class C { q: T } interface C<T> { p: T }`
  resolves `T` inside the class.
- **The static-member exception**, TypeScript 1.0 spec (April 2014) §3.4.1: a type
  parameter's scope is the whole declaration *except* static members. Upstream
  reports `Static_members_cannot_reference_class_type_parameters` and returns
  **nil**, not the symbol. The checker has no diagnostics yet (`bd tsr-5e7.6`), so
  only the `nil` is ported — answering the type parameter would be answering a
  question upstream refuses to answer.

### The alternative that was rejected: no meaning parameter

A class's `members` table holds its properties and methods as well as its type
parameters. Consulting it without a meaning would resolve `T` in
`class C<T> { m() { return T } }` — a *value* position — to the type parameter,
which upstream never does.

The tempting argument for skipping the filter is that it is currently invisible:
`get_type_of_symbol` of a type parameter is `errorType`, which prints `any`, and
so does an unresolved name. It was rejected because the two are different claims
and the difference surfaces the moment `getTypeOfFuncClassEnumModule` or a
diagnostic exists. `meaning` is therefore a parameter, exactly as upstream's is.

**What that cost:** a signature change on a function four modules call.
`tsr-checker/src/declared.rs` and `tsr-checker/src/expressions.rs` have moved to
`resolve_name` with `SymbolFlags::TYPE` and `SymbolFlags::VALUE`, which are the
meanings upstream passes for a type reference and an identifier expression. That
is what makes the 2,330 lines above reachable at all: the arm answers, and now
something asks it.

**`resolve` is gone.** It survived one commit as a shim delegating with an empty
meaning — upstream's `meaning == 0`, for which `r.lookup` returns nothing
(`nameresolver.go:423`), so every meaning-gated arm is skipped — because deleting
it revealed **two more callers than anyone had counted**, in
`tsr-conformance/src/types_producer.rs`. Choosing a meaning there is a judgement
about what the baseline producer prints at each site, not a mechanical
substitution, and converting them blind would have moved the producer underneath a
corpus measurement in flight; a number from a resolver that changed mid-run is not
interpretable. Once that run finished, `types_producer.rs` moved to
`SymbolFlags::TYPE` at its type-reference site — mirroring `declared.rs` line for
line, which is the property that instrument needs — and `resolve` was deleted.

A meaning-less resolver is not a harmless convenience. It answers a question
upstream never asks, and the failure mode is a caller nobody re-checks quietly
getting the wrong symbol years later. That is why it went rather than being kept
"just for the producer".

**Why the migration was safe to do piecemeal:** the widening is strictly additive.
`resolve_name` consults the same `locals` tables in the same order with no
meaning filter on them, so every answer `resolve` gave, `resolve_name` gives; the
only new answers come from the members arm, which is gated on
`meaning & SymbolFlags::TYPE`. Measured rather than asserted: moving both checker
call sites changed exactly one test result in `tsr-checker/tests/types.rs`, and
that one was the assertion pinning the *old* inherited-members answer.

### What the corpus said

Measured 2026-08-05 at `c8bf249`, which carried the members walk but **not** the
call-site change, so it prices the inherited-members half alone:

| suite | before | after |
|---|---:|---:|
| `binder_symbols` | 8,292/8,459 (98.03%) | **unchanged, to the case** |
| `printer_round_trip` | 11,681/11,737 | unchanged |
| `parser_typescript` | 5,000/5,031 | unchanged |
| `checker_types` cases | 596 | **602** |
| `checker_types` gradient | 36.17% | **36.23%** |

`binder_symbols` being *exactly* flat was a stated prediction with a real chance
of failing — a resolution change that had accidentally disturbed a symbol table
would have shown up here — so it is evidence rather than a formality.

The gradient moved ~290 lines against a histogram prediction of at most ~1,184
(886 "no such property on a receiver we typed" plus 298 on a `this`). Landing well
under the ceiling is the expected shape, because many of those lines sit in cases
that fail for other reasons too. **The slice did what the measurement said it
would and no more**, which is worth more than a surprise would have been: it is
the histogram earning its credibility. The type-parameter half is not in these
numbers — it became reachable only at `2c9fae5`.

One piece of arithmetic that is easy to get backwards, and was: `VALUE & TYPE` is
**not** empty — it is `CLASS | ENUM | ENUM_MEMBER`. A value reference therefore
does enter the members arm; it is the *filter*, not the arm, that excludes a type
parameter. That is upstream's `meaning & SymbolFlagsType` exactly, and no class or
interface members table can hold a `CLASS` or `ENUM` symbol anyway, so the arm is
empty in practice for a value lookup.

### What is deliberately still missing

`resolve_name` has two arms. Upstream's loop has a dozen: module and namespace
exports, enum members, `arguments`, a function expression's own name, `infer T`,
decorator relocation, computed property names, `ExpressionWithTypeArguments` in a
heritage clause, and a globals lookup at the end. Each is a **miss** here rather
than a wrong answer, because this change only ever adds a table to consult.

Two further limits, stated rather than left to be found:

- **The `locals` lookup is not meaning-filtered**, where upstream's is. That is
  the pre-existing behaviour and changing it is a separate question with its own
  regression surface — filtering would, among other things, stop an
  `import X = Y` alias resolving for a value reference, since `SymbolFlags::ALIAS`
  is not in `SymbolFlags::VALUE` and nothing here follows aliases yet
  (`bd tsr-y4u.12`).
- **A source file's `locals` are still consulted.** Upstream skips them
  (`!ast.IsGlobalSourceFile(location)`) because a script's top-level declarations
  are merged into the global table and found there instead. There is no global
  table here, so skipping them would resolve nothing at all. Unchanged, and it
  goes away with `bd tsr-9or.1`.

### How the ordering within one location is *not* tested

Upstream reads `location.Locals()` before the class arm, and so does
`resolve_inner`. That ordering is **unobservable here**: a class is
`IsContainer` without `HasLocals` (`GetContainerFlags`, mirrored in
`container.rs`), so no node ever owns both tables, and swapping the two turns no
test red. It is written in upstream's order because that is upstream's order, and
it is recorded here rather than pinned by a test that could not bite.

Shadowing — `class C<T> { m<T>(p: T) {} }` resolving to the *method's* `T` — is a
property of the outward walk stopping at the first hit, not of the intra-location
order, and *that* is tested: making a `locals` hit not win outright turns it red,
along with `lexical_resolution_walks_outward_and_stops_at_the_nearest_binding`.

### Tests, and the mutation each one answers

Seven in `tests/bind.rs` and three in `tsr-checker/tests/members.rs`, each
verified red under a specific mutation. `binder_symbols` is not evidence for any
of them — nothing about *binding* changed here, which is also the prediction that
suite should confirm by staying exactly flat.

| test | mutation that turns it red |
|---|---|
| `a_class_type_parameter_resolves_from_a_member_annotation` | remove the class arm; remove the meaning filter (the value-meaning contrast goes) |
| `an_interface_type_parameter_resolves_from_a_member_annotation` | the same two |
| `a_class_expression_type_parameter_resolves_too` | remove the class arm |
| `a_value_reference_does_not_find_a_type_parameter` | remove the meaning filter |
| `a_static_member_cannot_reference_the_class_type_parameter` | remove the §3.4.1 rule |
| `a_merged_interfaces_type_parameter_is_not_visible_in_the_class` | remove `isTypeParameterSymbolDeclaredInContainer` |
| `a_methods_own_type_parameter_shadows_the_classs` | make a `locals` hit not win outright |

The three in the checker are the ones that matter for *this* change, and the
reason is the trap the whole section is about. **The binder was binding `T`
correctly the entire time**, so any test asserting through the symbol would have
passed before the arm existed — passed for the wrong reason. These assert the
printed type of a member, which was `any` before:

| test | fixture | before | after |
|---|---|---|---|
| `a_class_type_parameter_resolves_in_a_member_annotation` | `class C<T> { p: T }`, `class C<T, U> { p: U }` | `any` | `T`, `U` |
| `an_interface_type_parameter_resolves_in_a_member_annotation` | `interface I<T> { p: T }` | `any` | `T` |
| `a_type_parameter_shadowed_by_an_outer_declaration_still_wins` | `type T = string;` + `class C<T> { p: T }` | **`string`** | `T` |

Each is red under two independent mutations: removing the class arm, and passing
the wrong meaning at the call site. The third is the one worth keeping if only
one could be: without the arm the walk sails past the class and reaches the file's
`type T = string`, so the answer is not a gap but a **wrong type that looks
right** — the failure mode the `errorType` discipline exists to prevent.
