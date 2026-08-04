# ADR-0025 — An export specifier is an export, and there is no modifier on it to find

Status: accepted
Date: 2026-08-05
Upstream pinned at `5b1047d10`.
Follows [ADR-0024](0024-static-members-and-block-scoped-declarations.md).

## Context

After [ADR-0024](0024-static-members-and-block-scoped-declarations.md), 92 TS2300
over-reports remained, and 52 of them were aliases: `ExportSpecifier` (26),
`ImportSpecifier` (20), `ImportClause` (5), `ImportEqualsDeclaration` (1). The near
match between the export and import counts was the clue — we report a redeclaration
on *every* declaration involved (`binder.go:259`), so one collision between an
import and an export is counted twice, once at each position.

`compiler/es6ExportEqualsInterop`'s `main.ts` declares both halves:

```ts
import { a as a1 } from "interface";   // line 62
export { a as a1 } from "interface";   // line 85
```

Upstream's `declareModuleMember` routes an alias by **node kind**
(`internal/binder/binder.go:376-380`):

```go
if symbolFlags&ast.SymbolFlagsAlias != 0 {
    if node.Kind == ast.KindExportSpecifier || (node.Kind == ast.KindImportEqualsDeclaration && hasExportModifier) {
        return b.declareSymbol(ast.GetExports(container.Symbol()), container.Symbol(), node, ...)
    }
    return b.declareSymbol(ast.GetLocals(container), nil, node, ...)
}
```

An `ExportSpecifier` goes to the container's **exports**, unconditionally. Every
other alias — import clause, import specifier, namespace import, plain
`import x = …` — goes to its **locals**.

Our `is_exported_from_container` decided the same question by asking whether the
node carried an `export` modifier. **An export specifier does not carry one.** The
keyword belongs to the `export { … }` declaration two levels above the specifier
that gets the symbol, so the test returned false and the specifier was filed as a
local named `a1` — where it met the import's local `a1`.

### The forcing constraint

This is the fourth wrong-table defect in three commits, and the first one whose
correction moved a conformance suite: `binder_symbols` **8,278 → 8,282**. That
matters because ADR-0024 recorded, as a warning, that the previous three moved it
by exactly zero and that 97.98% was therefore not evidence the tables were right.
The +4 confirms the suite *can* see a table defect — it just could not see those
three. It is a weak instrument, not a blind one, and this pins down which.

## Decision

Test the node kind, as upstream does: an `ExportSpecifier` is exported from its
container, without consulting modifiers or the ambient export context.

The container gate is unchanged and still correct — upstream reaches this branch
only through `declareModuleMember`, i.e. only when the container is a
`ModuleDeclaration` or an external-module source file, which is what the existing
`match` on the container's kind already expresses.

## Alternatives

**Make `has_export_modifier` walk up to the `ExportDeclaration` and find the
keyword there.** Rejected: it would get the right answer for the wrong reason, and
only by accident. Upstream never looks for a modifier on this path, and the
equivalence would break on the first construct where an export specifier exists
without an `export` keyword above it in the form this walk expects. Copying
upstream's predicate is cheaper than reconstructing one that happens to agree.

**Route it in `classify` alongside the type-parameter and static-member special
cases.** Rejected as the wrong location: this is not a question about which *kind*
of table (`Destination`), it is upstream's alias branch of `declareModuleMember`,
which `is_exported_from_container` already models. Adding it to `classify` would
have spread one upstream function across two of ours.

## Consequences

- `export { x }` with no module specifier is also affected, and correctly so. It
  reads like it should be a local, and is not: the export half is a separate symbol
  in the container's exports — that is what makes `M.x` reachable — and the local it
  aliases already exists from wherever `x` was declared. The alias path deliberately
  creates **one** symbol, not the local/export pair a non-alias export gets, which
  is already documented at that call site.
- `ImportEqualsDeclaration` with an `export` modifier still reaches exports through
  the existing modifier test, matching the second half of upstream's condition. It
  is not special-cased, because for that kind the modifier genuinely is on the node.
- Positions in the remaining buckets shifted: `ExportSpecifier` 26 → 1 and
  `ModuleDeclaration` 8 → 5, because those collisions had the same cause seen from a
  different declaration.

## How we would know this was wrong

- The unit test `an_export_specifier_is_an_export_not_a_local`, verified to fail
  with the binder change reverted.
- `binder_symbols` is now known to be *partially* sensitive to this class of defect
  — it moved +4 here and 0 for the three defects in ADR-0023 and ADR-0024. So a
  future table fix that moves it by zero still proves nothing, and the unit test
  remains the real gate. The lesson of ADR-0024 stands; this ADR narrows it rather
  than overturning it.

## Measured effect

| | over-reports | `diagnostics` | `binder_symbols` |
|---|---|---|---|
| after ADR-0024 | 92 | 78/5,488 | 8,278/8,449 |
| export specifiers are exports | **34** | **79/5,488** | **8,282/8,449** |

`ExportSpecifier` 26 → 1, `ImportSpecifier` 20 → 0, `ImportClause` 5 → 0,
`ImportEqualsDeclaration` 1 → 0.

Across the three ADRs in one session: **4,723 → 34** TS2300 divergences by construct
(4,350 → 34 on that tool's own measure), `binder_symbols` 8,278 → 8,282,
`diagnostics` 79 → 79 — the same number it started at, having dipped to 77 and
recovered, for the reasons ADR-0023 sets out.

### Perf

Bench and RSS were measured by the method
[`performance.md`](../architecture/performance.md) now prescribes — three readings
of the change *and* three of the baseline, same session — because a single
comparison against a recorded figure cannot distinguish a small regression from
this machine's noise:

| | parse+bind `checker.ts` |
|---|---|
| baseline (`4000cbe`) | 25.78 / 25.96 / 25.61 ms |
| with this change | 25.64 / 25.89 / 25.41 ms |

The change is neutral. Note that both sets read *above* the 24.98 ms measured
earlier the same session at load 0.11 — which is machine drift, not a regression in
either, and is exactly why the baseline was re-measured rather than assumed. RSS
15,052 KiB on three consecutive readings, inside the documented range.

## What is left

34 over-reports, no longer dominated by one mechanism: `ClassExpression` (8),
`ClassDeclaration` (7), `ModuleDeclaration` (5), `VariableDeclaration` (5),
`ExportAssignment` (2), `FunctionDeclaration` (2), and one each of
`ExportSpecifier`, `InterfaceDeclaration`, `MethodDeclaration`, `MethodSignature`,
`PropertySignature`. `bd tsr-y4u.19`. The 15 class cases are the largest remaining
group and the obvious next step; from here the tail is small enough that the
per-construct tool may stop being the right instrument.
