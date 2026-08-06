# Template expressions — the refusal re-tested and CONFIRMED with a number

Fifth session. `STATUS.md` §5 refuses the `TemplateExpression` row at 1,036
lines on the grounds that *"the cheap leg is **not separable** — upstream's
`evaluate` is a syntactic folder consulting no types"*. `depend.rs` at HEAD
reads the row at **1,890 lines, want-any 8 (0.4%)**, and the rules say a
refusal quoted on a stale number gets re-tested. It was.

## The mechanism

`checkTemplateExpression` (`checker.go:7976`) has three outcomes in order:

1. `evaluate(node, node).Value` folds the template to a constant string —
   answer a **fresh string literal** (`checker.go:7995`);
2. a `const` context, a template-literal context, or a template-literal
   contextual type — answer a **template literal type** (`:7997`);
3. otherwise — answer **`string`** (`checker.go:8000`).

## The measurement — `examples/tmplgap.rs`

The probe partitions the row by **what the baseline wants**, and folds each
template syntactically so the folder's own reach is measured rather than
assumed. C1 = 0, C2 exact, 1,036 classified.

| bucket | lines |
|---|---:|
| want a literal, this probe **cannot** fold it | 311 |
| want `string`, unfoldable — the cheap leg alone | 295 |
| want a literal, and this probe folds it **exactly** | 220 |
| want `string`, **and the folder would fold it** — the conflict | 151 |
| want a literal, folded **differently** | 59 |

## Both candidate designs measure ~1:1 gained against wrong

- **Cheap leg alone** (always `string`): converts 295 + 151 = **446**, and
  turns 220 + 59 + 311 = **590** gaps into wrong lines. **0.76 gained per
  wrong.**
- **Folder plus `string` fallback**: converts 295 + 220 = **515**, and turns
  151 + 59 + 311 = **521** into wrong. **0.99 gained per wrong.**

Against the **2.1 / 2.5 / 2.7** wrong-per-right that refused the module-object
item, qualified naming and `removeSubtypes`, both designs are worse than every
refusal on the board.

**The refusal STANDS, and its stated grounds are now a measurement rather than
an argument.** "Not separable" was correct and understated: the two legs do not
merely fail to separate, they **interleave** — 151 lines want the cheap answer
where the folder fires, and 59 more want a literal the folder gets wrong. A
design that stacks them cannot pick up one without dropping the other.

## What would overturn it

The 311 unfoldable-literal lines are the folder's real cost, and their head is
arithmetic and boolean spans — `` `s${1 + 1} - ${"S"} - ${!false}` `` — which
upstream's `evaluate` folds and this probe does not. A **complete** constant
folder (arithmetic, booleans, const-enum members, nested templates) would move
lines out of that 311 and out of the 59, changing both ratios. That is the only
route back, and it is a folder, not an arm. Nobody should re-derive the cheap
leg again.
