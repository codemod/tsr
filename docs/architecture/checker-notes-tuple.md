# Tuple type nodes — the plain leg

Registered at `c72ebf2` before any code, per `docs/conventions.md`. The goal
this session is steered by is the **70% gradient**, which needs +6,303 lines
from 68.68%; no single row on the board is that big, so this page is one of
several and says so.

---

## 1. Why this row, out of the board

`examples/depend.rs` at `c72ebf2` ranks the gap's roots. Property access is
still the largest row at 10,494, but **3,739 of its reachable lines are
"the property has no type"** — the property's own *declaration* gaps somewhere
else, which `bd tsr-mcd` already established is a symptom count and not an
item. `depend.rs`'s own advice is to follow those to the **type-node** roots
instead, and there the board reads:

| type-node root | lines | want `any` | note |
|---|---:|---:|---|
| `TypeReference` (no dependency) | 3,890 | 0.9% | **top-1 51.1%** — one case |
| `TupleType` | **2,141** | **0.0%** | 173 cases, top-1 23.4% |
| `ArrayType` | 2,137 | 0.2% | **top-1 93.6%** — one case |
| `TypeQuery` | 1,728 | 7.4% | |
| `UnionType` | 1,334 | 1.3% | |
| `ConstructorType` | 1,084 | 1.7% | |

**`TupleType` is the only large one that is neither ceiling nor a single
case**: a 0.0% `want-any` share, 173 cases, and a top case holding under a
quarter. The two rows above and below it are each dominated by one file.

## 2. The leg, and the evidence that it separates

`declared.rs` gaps tuples deliberately, and its stated reason is precise:
upstream reaches them through `getTypeFromArrayOrTupleTypeNode` with
`globalTupleType` and **a per-element flags model — `optional`, `rest`,
`variadic` — that has no counterpart here**, so *"a half-ported tuple would
answer plausible wrong lines for the rest."*

That is an argument against porting the *modifiers*, not against porting the
plain form. Counted over every tuple the baselines print
(`>x : [...]` lines across `compiler/` and `conformance/`):

```
  plain, no modifier and no label   966   74.9%
  rest, optional or labelled        323   25.1%
```

So the plain leg is three quarters of the printed population, and the modifier
model the comment refuses is the other quarter — which **gaps**, exactly as it
does today. This is the same shape as `&&` separating from `||`
(`checker-notes-armsplit.md` §3.1): the expensive machinery is real and it
guards a minority of the row.

## 3. What will be built

`get_type_from_tuple_type_node`, beside the array arm it shares an upstream
function with:

- every element resolved through `get_type_from_type_node`; **a gap in any
  element gaps the whole tuple**, the rule the array arm and
  `get_instantiated_type_reference` already use, because `[Unported, string]`
  is not `[any, string]`;
- **any element that is a `NamedTupleMember`, an `OptionalType` or a
  `RestType` gaps the whole tuple** — the modifier model stays unported and
  refuses rather than approximating;
- printed `[A, B]`, and `[]` for the empty tuple, which is what the baselines
  record;
- `readonly [A, B]` when the parent is a `readonly` type operator, read the
  same way the array arm reads it;
- **interned on the element list**, so `[number, string]` written twice is one
  type. Upstream gets this from `createTypeReference` on a tuple target; there
  is no target symbol here, so the intern map is keyed on the elements
  directly. Identity matters for the same reason it did for `C<number>`: a
  union of two spellings must collapse.

The type carries **no members**, so `t[0]`, `t.length` and destructuring stay
gaps — they are gaps today and nothing about them changes. That is the whole
of what makes this safe: today the tuple *node* answers `errorType` and every
consumer of it gaps, so the only lines that can move are the ones that render
the tuple itself.

## 4. The bar

> **KEEP** if **net ≥ +800**, **lost ≤ 30**, **fewer cases regress than
> finish**, and **Δwrong = −Δgap − Δright ≤ Δright ÷ 3**.
> **REVERT** otherwise.

The loss allowance is not zero, unlike the parenthesisation build's, because
this change can legitimately turn a *gap* into a *wrong* line without the rule
being wrong: a consumer downstream of a tuple that previously gapped now
receives a real type and may answer confidently and wrongly for reasons that
belong to that consumer. Above 30 that reading stops being credible.

**Prediction: +800 to +2,000.** The row is 2,141 gap lines rooted at a tuple
node, of which ~75% should be plain, plus whatever downstream "the property
has no type" lines a resolved tuple annotation unblocks.

**How this would be shown wrong:** if the gain is concentrated in
`compiler/genericDefaults` (the 23.4% top case), the row was never the
173-case population measured here.
