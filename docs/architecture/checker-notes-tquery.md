# Checker notes — `typeof x` in type position (`TypeQuery`)

`bd tsr-4sc.10`. Fourth session, 2026-08-06. Board item at score 680
(STATUS.md §4.2, measured at `b00738d`).

## 1. The population, sized by the mechanism and not the row

`depend.rs` publishes the `TypeQuery` root row at **1,728 gap lines** (fresh run
at this tree, 261 cases, top-1 7.5%). `examples/tquery.rs` — committed with this
page — classifies every one of those lines by what
`getTypeFromTypeQueryNode` (`checker.go:24102`) would have to do:

```
  1105  want-any  49  identifier, name types TODAY
   347  want-any  63  identifier, name still gaps
   148  want-any  15  qualified depth 1, base still gaps
    74  want-any   0  qualified depth 1, base types TODAY
    20  want-any   0  typeof this — refuses
    10  want-any   1  instantiation expression (type args) — refuses
    10  want-any   0  qualified depth 2, base types TODAY
     8  want-any   0  qualified depth 2, base still gaps
     6  want-any   0  qualified depth 3, base types TODAY

  C1 construction 0 violations · C2 buckets sum 1728 = total · C3 matches depend.rs
```

The mechanism's own population is therefore **~1,146 lines**: 1,056 net
(identifier, types today, ex-`want-any`) plus ~90 qualified-with-typing-base.
The other 582 convert nothing today whatever this arm does — 347 move their
gap to the name, and 176 are refused forms, 49+63+15+1 are ceiling.

A caveat the probe cannot see: a line whose *root* types after this arm may
still gap **downstream** of the root (the walk stops at the first typed
dependency, not at the line). That is exactly why the observed conversion band
in STATUS.md §4.1 runs 15–57% of a sized population, and why the bar's floor
below is set well under 1,146.

## 2. The upstream mechanism, read before writing

`getTypeFromTypeQueryNode` (`checker.go:24102`):

```go
t := c.checkExpressionWithTypeArguments(node)
links.resolvedType = c.getRegularTypeOfLiteralType(c.getWidenedType(t))
```

`checkExpressionWithTypeArguments` (`checker.go:10637`), for a `TypeQueryNode`:

- `isThisIdentifier(exprName)` → `checkThisExpression` — **refused here**
  (`typeof this` is 20 lines and the `this` expression arm answers per-container
  types this port only partially has);
- otherwise `checkExpression(exprName)` — an `Identifier` or `QualifiedName`.
  Upstream's `checkExpression` dispatches `KindQualifiedName` to
  `checkQualifiedName`, which is one call into
  `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11244`) — the
  function `members.rs` already ports for `PropertyAccessExpression`;
- `getInstantiationExpressionType` (`checker.go:10660`) is a **no-op when the
  node has no type arguments** (`typeArguments == nil → return exprType`).
  `typeof f<string>` (instantiation expressions) is a real mechanism — signature
  filtering by arity plus per-signature instantiation — and is **refused
  whole-construct** rather than approximated: 10 lines.

Divergence accepted, stated rather than hidden: this port has no general
`getWidenedType` (`checker.go:18355`). The types `check_expression` returns for
an entity name come from `get_type_of_symbol`, which widens variable-like
declarations at the declaration (`get_widened_type_for_variable_like_declaration`),
so the residual work of `getWidenedType` here is fresh-literal regularisation —
which `get_regular_type_of_literal_type` performs. If a corpus line shows a
`typeof` answer that needed object-literal widening *at the query*, that is
this divergence and this paragraph is where it is owned.

## 3. The keep/revert bar — registered before the code exists

Measured over the corpus with `casedelta.rs` and `wrongdelta.rs` across a
`git stash` pair at this tree.

1. **Net floor:** keep only if **net gained ≥ 350 lines**. 350 is ~33% of the
   1,056-line conditioned population — conditioned on "the name already types",
   so it should run *above* the 15–57% band's floor; if it cannot clear a third,
   the premise that the row is served by `check_expression` is wrong.
2. **Lost/gained rule — and this leg's denominator is empty by construction.**
   A line that is right today cannot pass through a `TypeQuery`, because the
   node answers `errorType` unconditionally and error poisons every dependent
   answer; there are no right lines for this arm to break *through that path*.
   Per `docs/conventions.md` ("a bar whose denominator the change cannot
   produce is not a bar"), the leg is not quoted as evidence; the honest form
   is absolute: **lost lines == 0**, and any loss is proof of a wrong rule
   (a cascade through a path not modelled here), to be diagnosed, not priced.
3. **Case-regression rule:** **0 cases regress** (per-case `casedelta`).
4. **Gap→wrong leg:** `wrongdelta` attributes every gap→wrong flip; keep only
   if **gained ≥ 3 × new wrong**. The mechanism fires on lines that are gaps
   today, so its failure mode is a confident wrong answer where upstream's
   widening or `this`-typing differs — the leg `casedelta` cannot see.

If the bar fires: first hypothesis is the build is wrong, second is the bar's
premise is wrong, there is no third. Overriding requires evidence independent
of this page's author, recorded loudly (commit, issue, STATUS.md, here).

**How this would be shown wrong** (the falsifier): a case where
`typeof x` must print differently from `get_type_of_symbol(x)`'s printed form —
e.g. a fresh object literal widening at the query, or upstream's shortest-path
node builder printing an alias name this port cannot reach. Those show up in
leg 4 as gap→wrong concentrated in `typeof`-annotated declarations.

## 4. What is deliberately not built, and what owns it

| form | lines | owner |
|---|---:|---|
| `typeof this` | 20 | `checkThisExpression` in type position — future `this` work |
| instantiation expressions `typeof f<T>` | 10 | `getInstantiationExpressionType`; needs arity filter + `getSignatureInstantiation` plumbing |
| qualified names whose base gaps | 156 | whatever gaps the base (mostly namespace/import machinery) |
| identifier still gaps | 347 | the name's own root, elsewhere on the board |

The `QualifiedName` twin **is** built: it is one dispatch line into the
member-lookup path `members.rs` already has, and refusing it would orphan the
90 base-types-today lines for no saving.

## 5. Scored against the bar — it FIRED once, and the answer was a mechanism

All measurements are `casedelta.rs`/`wrongdelta.rs` over the same
`git stash` pair; the wrong totals are same-probe, same-pair.

**First build** (the arm exactly as §2 describes, nothing else):

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ 350 | **+1,053**, 0 lost | pass |
| 2 | lost == 0 | 0 | pass |
| 3 | 0 case regressions | 0, +33 pass | pass |
| 4 | gained ≥ 3 × new wrong | 1,053 vs **+1,341** | **FIRED** |

Per the registered rule, first hypothesis: the build is wrong. It is not —
the computed types match upstream's semantics. Second hypothesis: the bar's
premise is wrong. Also no — leg 4 was designed for exactly this and caught it.
The finding is a **mechanism boundary**: 1,059 of the 1,341 keep `typeof X` in
the *wanted* text. Upstream's `.types` writer renders a function's signature
from its **declaration** — `signatureToSignatureDeclarationHelper` →
`serializeTypeForDeclaration`'s reuse branch keeps the written `typeof a`
node — while this port printed the resolved structure. The reuse condition
(the node's type equals the computed type) holds **by construction** for the
very annotation the type was computed from.

An intermediate attempt — refuse `typeof` over a parameter symbol — cut the
damage to +1,084 against +1,024 and still failed, because the family is not
"parameters": it is *every* `typeof` written in a signature's parameter or
return annotation (`typeof actionA`, `typeof foo` over ambient functions).
The narrowing was removed in favour of the mechanism it was approximating:
[`Parameter::written_text`] / [`Signature::written_return`] carry the written
`typeof` text, both signature printers prefer it, and instantiation drops it
when substitution changes the type (upstream's reuse condition).

**Final build, same pair:**

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ 350 | **+1,958**, 0 lost | pass |
| 2 | lost == 0 | 0 | pass |
| 3 | 0 case regressions | 0, **+57 pass** | pass |
| 4 | gained ≥ 3 × new wrong | **1,958 vs +436 = 4.5×** | pass |

Δwrong for the pair: 42,110 → 42,533 (**+423** net; 436 new, 13 fixed).

Two things worth keeping:

- **The conversion exceeded the sized row** — +1,958 against a 1,728-line
  root population — because the reuse mechanism also converts lines whose
  `depend.rs` root was *not* the `TypeQuery` node (signature prints reached
  through object-literal members and initialisers). A population is a ceiling
  *for the row it was measured on*, not for a mechanism that turns out wider.
- **The residual 436 decompose into owned families**, none of them this
  arm's: `typeof Backbone.Model` printed `typeof Model` (no accessibility
  chains, `bd tsr-93f`); `typeof moduleA` printing the module's internal
  path-name (`checker-notes-symbols.md` §7's known form); `M2.Point` vs
  `Point` and `FuncType` vs its structure (alias/qualified naming); and
  `(a: number | string)` printing sorted — evidence that upstream's
  signature-position reuse covers **all** written annotations, not just
  `typeof`, filed as `bd tsr-5o2`. Wider *declaration* reuse stays refused
  (`bd tsr-a2c`, 9.1 lost-per-gained — a different position with a different
  denominator).
